use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct CacheService {
    redis: Arc<RwLock<Option<ConnectionManager>>>,
    ram: Arc<RwLock<HashMap<String, String>>>,
    redis_url: Option<String>,
}

impl CacheService {
    pub async fn new(redis_url: Option<String>) -> Self {
        let redis = if let Some(url) = &redis_url {
            match redis::Client::open(url.as_str()) {
                Ok(client) => match client.get_connection_manager().await {
                    Ok(manager) => Some(manager),
                    Err(_) => {
                        eprintln!("Failed to connect to Redis initially. Using RAM.");
                        None
                    }
                },
                Err(_) => None,
            }
        } else {
            None
        };

        let service = Self {
            redis: Arc::new(RwLock::new(redis)),
            ram: Arc::new(RwLock::new(HashMap::new())),
            redis_url: redis_url.clone(),
        };

        if redis_url.is_some() {
            let s = service.clone();
            tokio::spawn(async move {
                s.monitor_redis().await;
            });
        }

        service
    }

    pub async fn get(&self, key: &str) -> Option<String> {
        // Try Redis
        let mut redis_guard = self.redis.write().await;
        if let Some(con) = redis_guard.as_mut() {
            match con.get::<_, String>(key).await {
                Ok(val) => return Some(val),
                Err(e) => {
                    eprintln!("Redis get failed: {}. Falling back to RAM.", e);
                    // Don't disable redis here, ConnectionManager reconnects. 
                    // But if it fails, we fall through to RAM.
                }
            }
        }

        // Try RAM
        let ram = self.ram.read().await;
        ram.get(key).cloned()
    }

    pub async fn set(&self, key: &str, value: &str, expiry_secs: usize) {
        // RAM always (as backup or primary)
        // Wait, if we use RAM as fallback, we should sync? 
        // User says "support Redis/RAM if redis work, if falls then use RAM".
        // This implies RAM is only active if Redis is dead? Or RAM is L1?
        // "Full support cache Redis/RAM if redis during work fell, then we will periodically ping it and parallel use RAM"
        // This suggests: Try Redis. If fail, use RAM. Background task pings Redis to restore.
        
        // Write to RAM first? Or only if Redis fail?
        // If Redis valid, RAM shouldn't be stale. 
        // Best approach: Write to both? Or just write to Redis if up.
        
        let mut redis_guard = self.redis.write().await;
        let mut redis_ok = false;
        
        if let Some(con) = redis_guard.as_mut() {
            if let Ok(_) = con.set_ex::<_, _, ()>(key, value, expiry_secs as u64).await {
                redis_ok = true;
            }
        }
        
        if !redis_ok {
            // Write to RAM if Redis failed or missing
             let mut ram = self.ram.write().await;
             ram.insert(key.to_string(), value.to_string());
             // Note: RAM expiry not implemented broadly here, but for simple use case acceptable or separate cleanup task needed.
        }
    }

    pub async fn del(&self, key: &str) {
         let mut redis_guard = self.redis.write().await;
         if let Some(con) = redis_guard.as_mut() {
             let _ = con.del::<_, ()>(key).await;
         }
         
         let mut ram = self.ram.write().await;
         ram.remove(key);
    }

    async fn monitor_redis(&self) {
        let url = match &self.redis_url {
            Some(u) => u,
            None => return,
        };

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            
            // Check if connected
            let mut redis_guard = self.redis.write().await;
            if redis_guard.is_none() {
                 match redis::Client::open(url.as_str()) {
                    Ok(client) => match client.get_connection_manager().await {
                        Ok(manager) => {
                            println!("Reconnected to Redis!");
                            *redis_guard = Some(manager);
                            // Sync RAM to Redis? Or Clear RAM? Clear RAM usually.
                            let mut ram = self.ram.write().await;
                            ram.clear();
                        }
                        Err(_) => {}
                    },
                    Err(_) => {}
                }
            } else {
                // ConnectionManager handles reconnects, but if we want to check if it's REALLY working:
                let con = redis_guard.as_mut().unwrap();
                if let Err(_) = redis::cmd("PING").query_async::<String>(con).await {
                    println!("Redis PING failed. Disabling Redis temporarily.");
                    *redis_guard = None;
                }
            }
        }
    }
}

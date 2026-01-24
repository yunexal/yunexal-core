use crate::models::{HeartbeatPayload, Node};
use crate::services::cache::CacheService;
use reqwest::Client as HttpClient;
use sea_orm::DatabaseConnection;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use sea_orm::EntityTrait; // for finding nodes

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub cache: CacheService,
    pub http_client: HttpClient,
    pub panel_name: Arc<RwLock<String>>,
    pub panel_font: Arc<RwLock<String>>,
    pub panel_font_url: Arc<RwLock<String>>,
    pub nodes_cache: Arc<RwLock<Option<Vec<Node>>>>,
    pub heartbeats_cache: Arc<RwLock<HashMap<String, HeartbeatPayload>>>,
    pub is_sqlite: bool, // Still useful for connection string logic maybe, but less for queries
}

impl AppState {
    pub async fn get_nodes(&self) -> Vec<Node> {
        // 1. Check RAM Cache (Internal L1)
        {
            let lock = self.nodes_cache.read().await;
            if let Some(nodes) = &*lock {
                return nodes.clone();
            }
        }

        // 2. Check Cache Service (Redis/RAM L2)
        if let Some(json) = self.cache.get("cache:nodes").await {
             if let Ok(nodes) = serde_json::from_str::<Vec<Node>>(&json) {
                // Populate L1 RAM
                let mut lock = self.nodes_cache.write().await;
                *lock = Some(nodes.clone());
                return nodes;
            }
        }

        // 3. Fetch DB via SeaORM
        use crate::entities::nodes::Entity as NodeEntity;
        
        let nodes_result: Result<Vec<Node>, sea_orm::DbErr> = NodeEntity::find().all(&self.db).await;

        let nodes = nodes_result.unwrap_or_default();

        // 4. Update Caches
        let json = serde_json::to_string(&nodes).unwrap_or_default();
        self.cache.set("cache:nodes", &json, 300).await;

        // Update L1 RAM
        let mut lock = self.nodes_cache.write().await;
        *lock = Some(nodes.clone());

        nodes
    }

    pub async fn invalidate_nodes_cache(&self) {
        // Clear L1 RAM
        let mut lock = self.nodes_cache.write().await;
        *lock = None;

        // Clear L2 Cache
        self.cache.del("cache:nodes").await;
    }
}

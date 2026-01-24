use axum::{
    extract::{State, Path, Json},
    http::{HeaderMap, StatusCode},
};
use tracing::{info};
use crate::{state::AppState, models::{Node, HeartbeatPayload}};
use sea_orm::{EntityTrait, ActiveModelTrait, Set, IntoActiveModel};
use crate::entities::nodes;

pub async fn heartbeat_handler(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<HeartbeatPayload>,
) -> StatusCode {
    // [TRACE] Entry
    info!("[TRACE] -> heartbeat_handler triggered for ID: {}", id_str);
    
    let id = id_str;
    
    // Verify Token
    let auth_header = headers.get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    if let Some(token) = auth_header {
        info!("[TRACE] Token received: {}...", &token.chars().take(5).collect::<String>());
        let mut node_opt: Option<Node> = None;

        // 1. Try Cache
        let cache_key = format!("node:{}:cache", id);
        if let Some(json) = state.cache.get(&cache_key).await {
             info!("[TRACE] Node found in Cache");
             if let Ok(n) = serde_json::from_str::<Node>(&json) {
                 node_opt = Some(n);
             }
        } else {
             info!("[TRACE] Node NOT in Cache");
        }

        // 2. Fallback to Memory Cache (Avoid DB Hit)
        if node_opt.is_none() {
             let nodes_lock = state.nodes_cache.read().await;
             if let Some(nodes) = &*nodes_lock {
                 if let Some(n) = nodes.iter().find(|n: &&Node| n.id.to_string() == id) {
                     info!("[TRACE] Node found in Memory Cache");
                     node_opt = Some((*n).clone());
                 }
             }
        }

        // 3. Fallback to DB
        if node_opt.is_none() {
            info!("[TRACE] Fallback to DB Lookup for node: {}", id);
            
            if let Ok(uid) = uuid::Uuid::parse_str(&id) {
                node_opt = nodes::Entity::find_by_id(uid)
                    .one(&state.db)
                    .await
                    .unwrap_or(None);
            }

            // Cache result if found
            if let Some(ref n) = node_opt {
                info!("[TRACE] Node found in DB, caching...");
                if let Ok(json) = serde_json::to_string(n) {
                    state.cache.set(&cache_key, &json, 60).await;
                }
            } else {
                info!("[TRACE] Node NOT found in DB");
            }
        }

        let mut authorized = false;
        if let Some(node) = node_opt {
             if node.token == token {
                info!("[TRACE] Token MATCH - Authorized");
                authorized = true;
                // Update Version in DB if changed
                if node.version != payload.version {
                     info!("[TRACE] Updating version from {} to {}", node.version, payload.version);
                     
                     let mut active = node.clone().into_active_model();
                     active.version = Set(payload.version.clone());
                     if let Ok(_) = active.update(&state.db).await { }
                }

                // Update Heartbeat Cache
                let mut hb_lock = state.heartbeats_cache.write().await;
                hb_lock.insert(node.id.to_string(), payload.clone());
                info!("[TRACE] Heartbeat cached for node: {}", node.id);
             } else {
                 info!("[TRACE] Token MISMATCH. Received: {}, Expected: {}", token, node.token);
             }
        }

        if authorized {
            return StatusCode::OK;
        } else {
            return StatusCode::UNAUTHORIZED;
        }
    }

    StatusCode::UNAUTHORIZED
}

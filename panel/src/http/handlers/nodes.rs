use axum::{
    extract::{State, Path, Form, Query},
    response::{Redirect, IntoResponse},
    http::{HeaderMap, StatusCode},
};
use sea_orm::{QueryFilter, ColumnTrait};
use std::collections::HashSet;
use crate::{state::AppState, models::{Node, CreateNodeRequest, UpdateNodeRequest}};
use uuid::Uuid;
use askama::Template;
use crate::http::handlers::HtmlTemplate;
use sea_orm::{EntityTrait, ActiveModelTrait, Set, ActiveValue};
use crate::entities::{nodes, allocations};

#[derive(Template)]
#[template(path = "node_create.html")]
struct CreateNodeTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String, 
    panel_version: String,
    execution_time: f64,
    active_tab: String,
}

#[derive(Template)]
#[template(path = "node_edit.html")]
struct EditNodeTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String,
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    node: Node,
    found: bool,
    install_cmd: String,
    uninstall_cmd: String,
}

#[derive(Template)]
#[template(path = "node_setup.html")]
struct SetupNodeTemplate {
    panel_font: String,
    panel_font_url: String,
    panel_name: String,
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    node: Node,
    install_cmd: String,
    found: bool,
}

pub async fn create_node_page_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();
    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(CreateNodeTemplate {
        panel_name,
        panel_font,
        panel_font_url, 
        panel_version,
        execution_time,
        active_tab: "nodes".to_string(),
    })
}

pub async fn create_node_handler(
    State(state): State<AppState>,
    Form(payload): Form<CreateNodeRequest>,
) -> Redirect {
    // Validate Port
    if (payload.port >= 0 && payload.port <= 1023) || (payload.sftp_port >= 0 && payload.sftp_port <= 1023) {
        eprintln!("Blocked attempt to create node on restricted port");
        return Redirect::to("/nodes/new");
    }

    // Validate Collision
    if payload.port == payload.sftp_port {
        eprintln!("Daemon Port and SFTP Port cannot be the same");
        return Redirect::to("/nodes/new");
    }

    let id = Uuid::new_v4();
    let token = Uuid::new_v4().to_string();

    let new_node = nodes::ActiveModel {
        id: ActiveValue::Set(id),
        name: ActiveValue::Set(payload.name.clone()),
        ip: ActiveValue::Set(payload.ip.clone()),
        port: ActiveValue::Set(payload.port),
        token: ActiveValue::Set(token),
        sftp_port: ActiveValue::Set(payload.sftp_port),
        ram_limit: ActiveValue::Set(payload.ram_limit.unwrap_or(0)),
        disk_limit: ActiveValue::Set(payload.disk_limit.unwrap_or(0)),
        cpu_limit: ActiveValue::Set(payload.cpu_limit.unwrap_or(0)),
        version: ActiveValue::Set("".to_string()),
    };

    if let Err(e) = new_node.insert(&state.db).await {
        eprintln!("Failed to insert node: {}", e);
        return Redirect::to("/nodes");
    }

    // Process initial allocations if provided
    if let Some(ports_str) = &payload.allocation_ports {
        let ports = parse_ports(ports_str);
        
        for port in &ports {
            if *port >= 0 && *port <= 1023 {
                eprintln!("Blocked attempt to use restricted allocation port: {}", port);
                return Redirect::to("/nodes/new");
            }
        }

        let unique_ports: HashSet<i32> = ports.into_iter().collect();
        for port in unique_ports {
             if port >= 0 && port <= 65535 {
                let alloc = allocations::ActiveModel {
                     id: ActiveValue::Set(Uuid::new_v4()),
                     node_id: ActiveValue::Set(id),
                     ip: ActiveValue::Set(payload.ip.clone()),
                     port: ActiveValue::Set(port),
                     server_id: ActiveValue::NotSet,
                 };
                 let _ = alloc.insert(&state.db).await; 
             }
        }
    }

    // Invalidate Cache
    state.invalidate_nodes_cache().await;

    Redirect::to(&format!("/nodes/{}/setup", id))
}

pub async fn setup_node_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let host = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or("127.0.0.1:3000");
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone(); 
    let panel_version = env!("CARGO_PKG_VERSION").to_string();

    let node_opt = if let Ok(uid) = Uuid::parse_str(&id) {
        nodes::Entity::find_by_id(uid).one(&state.db).await.unwrap_or(None)
    } else {
        None
    };

    let (node, found, install_cmd) = match node_opt {
        Some(n) => {
            let cmd = format!("curl -sSL http://{}/install/{} | sudo bash", host, n.id);
            (n, true, cmd)
        },
        _ => (
            Node { 
                id: Uuid::default(), 
                name: "".to_string(), 
                ip: "".to_string(), 
                port: 0, 
                token: "".to_string(),
                sftp_port: 0,
                ram_limit: 0,
                disk_limit: 0,
                cpu_limit: 0,
                version: "".to_string()
            },
            false,
            "".to_string()
        ),
    };

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(SetupNodeTemplate {
        panel_font,
        panel_font_url, 
        panel_name,
        panel_version,
        execution_time,
        active_tab: "nodes".to_string(),
        node,
        found,
        install_cmd,
    })
}

pub async fn delete_node_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl axum::response::IntoResponse {
    if let Ok(uid) = Uuid::parse_str(&id) {
        let _ = nodes::Entity::delete_by_id(uid).exec(&state.db).await;
        
        // Invalidate Cache
        state.invalidate_nodes_cache().await;
    }
    
    // Return empty string with 200 OK
    ""
}

pub async fn edit_node_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now(); 
    let panel_version = env!("CARGO_PKG_VERSION").to_string();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();

    let node_res = if let Ok(uid) = Uuid::parse_str(&id) {
         nodes::Entity::find_by_id(uid).one(&state.db).await.unwrap_or(None)
    } else {
        None
    };

    let host = "127.0.0.1:3000";

    let (node_val, found, install_cmd, uninstall_cmd) = if let Some(n) = node_res {
        let install = format!("curl -sSL http://{}/install/{} | sudo bash", host, n.id);
        let uninstall = format!("systemctl stop yunexal-node-{} && rm -rf /etc/yunexal/node-{}", n.id, n.id);
        (n, true, install, uninstall)
    } else {
        (
            Node { 
                id: Uuid::default(), 
                name: "".to_string(), 
                ip: "".to_string(), 
                port: 0, 
                token: "".to_string(),
                sftp_port: 0,
                ram_limit: 0,
                disk_limit: 0,
                cpu_limit: 0,
                version: "".to_string()
            },
            false,
            "".to_string(),
            "".to_string()
        )
    };

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(EditNodeTemplate {
        panel_name,
        panel_font,
        panel_font_url,
        panel_version,
        execution_time,
        active_tab: "nodes".to_string(),
        node: node_val,
        found,
        install_cmd,
        uninstall_cmd,
    })
}

pub async fn update_node_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(payload): Form<UpdateNodeRequest>,
) -> Redirect {
    // Validate Ports (duplicated logic from create, could be shared)
    if (payload.port >= 0 && payload.port <= 1023) || (payload.sftp_port >= 0 && payload.sftp_port <= 1023) {
        eprintln!("Blocked attempt to update node to restricted port");
        return Redirect::to(&format!("/nodes/{}/edit", id));
    }
    if payload.port == payload.sftp_port {
        eprintln!("Daemon Port and SFTP Port cannot be the same");
        return Redirect::to(&format!("/nodes/{}/edit", id));
    }

    // Fetch existing node to update
    let uid = Uuid::parse_str(&id).unwrap_or(Uuid::default());
    let node_opt = nodes::Entity::find_by_id(uid).one(&state.db).await.unwrap_or(None);

    if let Some(node) = node_opt {
        let mut active: nodes::ActiveModel = node.into();
        active.name = Set(payload.name);
        active.ip = Set(payload.ip);
        active.port = Set(payload.port);
        active.sftp_port = Set(payload.sftp_port);
        active.ram_limit = Set(payload.ram_limit.unwrap_or(0));
        active.disk_limit = Set(payload.disk_limit.unwrap_or(0));
        active.cpu_limit = Set(payload.cpu_limit.unwrap_or(0));
        
        let _ = active.update(&state.db).await;
        
        // Invalidate Cache
        state.invalidate_nodes_cache().await;
    }
    
    Redirect::to("/")
}

pub async fn trigger_node_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let node_opt = if let Ok(uid) = Uuid::parse_str(&id) {
         nodes::Entity::find_by_id(uid).one(&state.db).await.unwrap_or(None)
    } else {
        None
    };

    if let Some(node) = node_opt {
        let url = format!("http://{}:{}/self-update", node.ip, node.port);
        let client = reqwest::Client::new();
        
        // Need to check what response type matches?
        // Assuming client is reqwest::Client
        let res = client.post(&url)
            .header("Authorization", &format!("Bearer {}", node.token))
            .send()
            .await;
            
        return match res {
            Ok(r) => {
                 if r.status().is_success() {
                     axum::response::Response::builder()
                        .status(200)
                        .body(axum::body::Body::from("Update initiated"))
                        .unwrap()
                 } else {
                     axum::response::Response::builder()
                        .status(500)
                        .body(axum::body::Body::from(format!("Node error: {}", r.status())))
                        .unwrap()
                 }
            },
            Err(e) => {
                axum::response::Response::builder()
                    .status(500)
                    .body(axum::body::Body::from(format!("Connection failed: {}", e)))
                    .unwrap()
            }
        };
    }
    
    axum::response::Response::builder()
        .status(404)
        .body(axum::body::Body::from("Node not found"))
        .unwrap()
}

fn parse_ports(input: &str) -> Vec<i32> {
    let mut result = Vec::new();
    let parts: Vec<&str> = input.split(',').collect();
    
    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() { continue; }
        
        if trimmed.contains('-') {
            let range_parts: Vec<&str> = trimmed.split('-').collect();
            if range_parts.len() == 2 {
                if let (Ok(start), Ok(end)) = (range_parts[0].trim().parse::<i32>(), range_parts[1].trim().parse::<i32>()) {
                    if start <= end {
                        for p in start..=end {
                            result.push(p);
                        }
                    }
                }
            }
        } else {
            if let Ok(p) = trimmed.parse::<i32>() {
                result.push(p);
            }
        }
    }
    result
}

#[derive(serde::Deserialize)]
pub struct DownloadQuery {
    token: Option<String>,
}

pub async fn download_node_agent(
    State(state): State<AppState>,
    Query(query): Query<DownloadQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    
    // Optional Auth: If token is provided, validate it. 
    // If NOT provided, we currently allow it to support legacy agent updates.
    // TODO: Enforce token in future versions.
    if let Some(token) = query.token {
        let exists = nodes::Entity::find()
            .filter(nodes::Column::Token.eq(&token))
            .one(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .is_some();

        if !exists {
            return Err(StatusCode::FORBIDDEN);
        }
    }

    let content = tokio::fs::read("public/yunexal-node").await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "application/octet-stream"),
            (axum::http::header::CONTENT_DISPOSITION, "attachment; filename=\"yunexal-node\""),
        ],
        axum::body::Body::from(content)
    ))
}

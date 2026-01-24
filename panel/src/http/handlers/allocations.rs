use crate::http::handlers::HtmlTemplate;
use crate::{
    models::{Allocation, CreateAllocationRequest, DeleteAllocationRequest, Node},
    state::AppState,
};
use askama::Template;
use axum::{
    extract::{Form, Path, Query, State},
    response::{IntoResponse, Redirect},
};
use serde::Deserialize;
use std::collections::HashSet;
use uuid::Uuid;
use sea_orm::{EntityTrait, QueryFilter, ColumnTrait, ActiveModelTrait, ActiveValue, QueryOrder, PaginatorTrait};
use crate::entities::{nodes, allocations};

#[derive(Template)]
#[template(path = "node_allocations.html")]
struct AllocationsTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String, 
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    node: Node,
    allocations: Vec<Allocation>,
    page: u32,
    has_more: bool,
}

#[derive(Deserialize)]
pub struct PaginationQuery {
    page: Option<u32>,
}

pub async fn allocations_page_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<PaginationQuery>,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();

    let page = params.page.unwrap_or(1);
    let limit = 50;

    let uid = Uuid::parse_str(&id).unwrap_or(Uuid::default());
    let node_opt = nodes::Entity::find_by_id(uid).one(&state.db).await.unwrap_or(None);

    if node_opt.is_none() {
        return Redirect::to("/nodes").into_response();
    }
    let node = node_opt.unwrap();

    let paginator = allocations::Entity::find()
        .filter(allocations::Column::NodeId.eq(uid))
        .order_by_asc(allocations::Column::Port)
        .paginate(&state.db, limit);
    
    let display_allocations = paginator.fetch_page(page as u64 - 1).await.unwrap_or_default();
    let total_pages = paginator.num_pages().await.unwrap_or(0);
    // Logic fix: if total_pages is 0 (empty), has_more is false. 
    // If page < total_pages, implies more.
    let has_more = (page as u64) < total_pages; 

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(AllocationsTemplate {
        panel_name,
        panel_font,
        panel_font_url,
        panel_version,
        execution_time,
        active_tab: "nodes".to_string(),
        node,
        allocations: display_allocations,
        page,
        has_more,
    })
    .into_response()
}

pub async fn create_allocations_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(payload): Form<CreateAllocationRequest>,
) -> Redirect {
    let ports = parse_ports(&payload.ports);
    let uid = Uuid::parse_str(&id).unwrap_or(Uuid::default());

    // Deduplicate
    let unique_ports: HashSet<i32> = ports.into_iter().collect();

    for port in unique_ports {
        // Enforce port restrictions server-side
        if port >= 0 && port <= 1023 {
            continue; // Skip system ports
        }

        if port >= 0 && port <= 65535 {
             let alloc = allocations::ActiveModel {
                 id: ActiveValue::Set(Uuid::new_v4()),
                 node_id: ActiveValue::Set(uid),
                 ip: ActiveValue::Set(payload.ip.clone()),
                 port: ActiveValue::Set(port),
                 server_id: ActiveValue::NotSet, // Null
             };
             let _ = alloc.insert(&state.db).await; // ON CONFLICT DO NOTHING behavior?
             // SeaORM returns Error on duplicate. We just ignore it.
        }
    }

    Redirect::to(&format!("/nodes/{}/allocations", id))
}

pub async fn delete_allocations_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(payload): Form<DeleteAllocationRequest>,
) -> Redirect {
    let ports_to_delete = parse_ports(&payload.ports);
    let uid = Uuid::parse_str(&id).unwrap_or(Uuid::default());

    for port in ports_to_delete {
         let mut query = allocations::Entity::delete_many()
            .filter(allocations::Column::NodeId.eq(uid))
            .filter(allocations::Column::Port.eq(port));
        
        if !payload.force {
             query = query.filter(allocations::Column::ServerId.is_null());
        }
        
        let _ = query.exec(&state.db).await;
    }

    Redirect::to(&format!("/nodes/{}/allocations", id))
}

fn parse_ports(input: &str) -> Vec<i32> {
    let mut result = Vec::new();
    let parts: Vec<&str> = input.split(',').collect();

    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.contains('-') {
            let range_parts: Vec<&str> = trimmed.split('-').collect();
            if range_parts.len() == 2 {
                if let (Ok(start), Ok(end)) = (
                    range_parts[0].trim().parse::<i32>(),
                    range_parts[1].trim().parse::<i32>(),
                ) {
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

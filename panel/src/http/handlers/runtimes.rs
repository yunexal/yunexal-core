use crate::http::handlers::HtmlTemplate;
use crate::models::{
    Image, Runtime
};
use crate::entities::{runtimes, images};
use crate::state::AppState;
use askama::Template;
use axum::{
    extract::{Form, Json, State},
    response::{IntoResponse, Redirect},
};
use uuid::Uuid;
use sea_orm::{
    EntityTrait, QueryOrder, ActiveModelTrait, Set, 
};
// Use crate entities
use crate::entities::prelude::*;

#[derive(Template)]
#[template(path = "runtimes.html")]
struct RuntimesTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String, // Added
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    // Using a tuple struct or wrapper for logic
    runtimes: Vec<RuntimeWithImages>,
}

struct RuntimeWithImages {
    runtime: Runtime,
    images: Vec<Image>,
    image_count: usize,
}

#[derive(Template)]
#[template(path = "runtime_create.html")]
struct CreateRuntimeTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String, // Added
    panel_version: String,
    execution_time: f64,
    active_tab: String,
}

#[derive(Template)]
#[template(path = "runtime_edit.html")]
struct EditRuntimeTemplate {
    panel_font: String,
    panel_font_url: String, // Added
    panel_name: String,
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    runtime: Runtime,
}

pub async fn edit_runtime_page_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();

    let runtime = Runtimes::find_by_id(id)
        .one(&state.db)
        .await;

    match runtime {
        Ok(Some(r)) => {
            let elapsed = start_time.elapsed();
            let execution_time = elapsed.as_secs_f64() * 1000.0;

            HtmlTemplate(EditRuntimeTemplate {
                panel_name,
                panel_font,
                panel_font_url,
                panel_version,
                execution_time,
                active_tab: "runtimes".to_string(),
                runtime: r,
            })
            .into_response()
        }
        _ => Redirect::to("/runtimes").into_response(),
    }
}

pub async fn update_runtime_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    Form(payload): Form<UpdateRuntimeRequest>,
) -> Redirect {
    let existing = match Runtimes::find_by_id(id).one(&state.db).await {
        Ok(Some(ex)) => ex,
        _ => return Redirect::to("/runtimes"),
    };

    let mut active: runtimes::ActiveModel = existing.into();
    active.name = Set(payload.name);
    active.description = Set(payload.description);
    active.color = Set(Some(payload.color));

    let _ = active.update(&state.db).await;

    Redirect::to("/runtimes")
}

pub async fn delete_runtime_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> impl IntoResponse {
    let _ = Runtimes::delete_by_id(id).exec(&state.db).await;

    let mut headers = axum::http::HeaderMap::new();
    headers.insert("HX-Redirect", "/runtimes".parse().unwrap());
    (headers, "Deleted")
}

#[derive(serde::Deserialize)]
pub struct CreateRuntimeRequest {
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub description: Option<String>,
    #[allow(dead_code)]
    #[serde(default = "default_color")]
    pub color: String,
}

#[derive(serde::Deserialize)]
pub struct UpdateRuntimeRequest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default = "default_color")]
    pub color: String,
}

fn default_color() -> String {
    "#007bff".to_string()
}

#[derive(Template)]
#[template(path = "image_create.html")]
struct CreateImageTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String, // Added
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    runtime_id: String,
}

#[derive(serde::Deserialize)]
pub struct CreateImageRequest {
    pub name: String,
    pub docker_images: String,
    pub description: Option<String>,
    pub startup_command: String,
    pub stop_command: String,
    #[serde(default)]
    pub requires_port: bool,
    pub log_config: String,
    pub config_files: String,
    pub start_config: String,
    // New Fields
    #[serde(default)]
    pub install_script: String,
    #[serde(default)]
    pub install_container: String,
    #[serde(default)]
    pub install_entrypoint: String,
    #[serde(default = "default_array_json")]
    pub variables: String,
}

fn default_array_json() -> String {
    "[]".to_string()
}

pub async fn create_image_page_handler(
    State(state): State<AppState>,
    axum::extract::Path(runtime_id): axum::extract::Path<Uuid>,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(CreateImageTemplate {
        panel_name,
        panel_font,
        panel_font_url,
        panel_version,
        execution_time,
        active_tab: "runtimes".to_string(),
        runtime_id: runtime_id.to_string(),
    })
}

pub async fn create_image_handler(
    State(state): State<AppState>,
    axum::extract::Path(runtime_id): axum::extract::Path<Uuid>,
    Form(payload): Form<CreateImageRequest>,
) -> Redirect {
    let id = Uuid::new_v4();

    let new_image = images::ActiveModel {
        id: Set(id),
        runtime_id: Set(runtime_id),
        name: Set(payload.name),
        docker_images: Set(payload.docker_images),
        description: Set(payload.description),
        startup_command: Set(payload.startup_command),
        stop_command: Set(payload.stop_command),
        requires_port: Set(payload.requires_port),
        log_config: Set(payload.log_config),
        config_files: Set(payload.config_files),
        start_config: Set(payload.start_config),
        install_script: Set(payload.install_script),
        install_container: Set(payload.install_container),
        install_entrypoint: Set(payload.install_entrypoint),
        variables: Set(payload.variables),
    };

    let _ = Images::insert(new_image).exec(&state.db).await;

    Redirect::to("/runtimes")
}

pub async fn runtimes_page_handler(State(state): State<AppState>) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();

    let runtimes_db = Runtimes::find()
        .order_by_asc(runtimes::Column::SortOrder)
        .order_by(runtimes::Column::Name, sea_orm::Order::Asc)
        .all(&state.db)
        .await
        .unwrap_or_default();

    let images_db = Images::find()
        .all(&state.db)
        .await
        .unwrap_or_default();

    // Group images by runtime
    let mut runtimes = Vec::new();
    for r in runtimes_db {
        let mut my_images: Vec<Image> = images_db
            .iter()
            .filter(|i| i.runtime_id == r.id)
            .cloned()
            .collect();
        my_images.sort_by(|a, b| a.name.cmp(&b.name));

        let count = my_images.len();
        runtimes.push(RuntimeWithImages {
            runtime: r,
            images: my_images,
            image_count: count,
        });
    }

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(RuntimesTemplate {
        panel_name,
        panel_font,
        panel_font_url,
        panel_version,
        execution_time,
        active_tab: "runtimes".to_string(),
        runtimes,
    })
}

pub async fn create_runtime_page_handler(State(state): State<AppState>) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(CreateRuntimeTemplate {
        panel_name,
        panel_font,
        panel_font_url,
        panel_version,
        execution_time,
        active_tab: "runtimes".to_string(),
    })
}

pub async fn create_runtime_handler(
    State(state): State<AppState>,
    Form(payload): Form<CreateRuntimeRequest>,
) -> Redirect {
    let id = Uuid::new_v4();

    // Determine sort_order - simple max+1
    // We can use a query to get MAX sort_order
    // But since SeaORM generic aggregate might be verbose, we can just invoke it.
    // Or just fetch all and take last. For few runtimes it's fine.
    
    // Better:
    // let max_order = Runtimes::find().order_by_desc(runtimes::Column::SortOrder).one(&state.db).await
    // This gives the one with highest sort order.
    
    let max_order_runtime = Runtimes::find()
        .order_by_desc(runtimes::Column::SortOrder)
        .one(&state.db)
        .await
        .unwrap_or(None);
        
    let next_order = match max_order_runtime {
        Some(r) => r.sort_order + 1,
        None => 1,
    };

    let new_runtime = runtimes::ActiveModel {
        id: Set(id),
        name: Set(payload.name),
        description: Set(payload.description),
        color: Set(Some(payload.color)),
        sort_order: Set(next_order),
    };

    let _ = Runtimes::insert(new_runtime).exec(&state.db).await;

    Redirect::to("/runtimes")
}

#[derive(serde::Deserialize)]
pub struct ReorderRuntimesRequest {
    pub order: Vec<String>, // list of ID strings
}

pub async fn reorder_runtimes_handler(
    State(state): State<AppState>,
    Json(payload): Json<ReorderRuntimesRequest>,
) -> impl IntoResponse {
    // We update each runtime with new sort_order
    for (index, id_str) in payload.order.iter().enumerate() {
        if let Ok(uid) = Uuid::parse_str(id_str) {
            let active_model = runtimes::ActiveModel {
                id: Set(uid),
                sort_order: Set((index + 1) as i32),
                ..Default::default()
            };
            // Use update since we only update sort_order (id is PK)
            // But we must construct it properly.
            // Actually, we can just use `update` if PK is set.
            let _ = active_model.update(&state.db).await;
        }
    }
    axum::http::StatusCode::OK
}

// Image Edit

#[derive(Template)]
#[template(path = "image_edit.html")]
struct EditImageTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String, // Added
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    runtime_id: String,
    image: Image,
}

pub async fn edit_image_page_handler(
    State(state): State<AppState>,
    axum::extract::Path((runtime_id, image_id)): axum::extract::Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();

    let image = match Images::find_by_id(image_id).one(&state.db).await {
        Ok(Some(img)) => img,
        _ => return Redirect::to("/runtimes").into_response(),
    };

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(EditImageTemplate {
        panel_name,
        panel_font,
        panel_font_url,
        panel_version,
        execution_time,
        active_tab: "runtimes".to_string(),
        runtime_id: runtime_id.to_string(),
        image,
    }).into_response()
}

pub async fn update_image_handler(
    State(state): State<AppState>,
    axum::extract::Path((_runtime_id, image_id)): axum::extract::Path<(Uuid, Uuid)>,
    Form(payload): Form<CreateImageRequest>,
) -> Redirect {
    let existing = match Images::find_by_id(image_id).one(&state.db).await {
         Ok(Some(i)) => i,
         _ => return Redirect::to("/runtimes"),
    };

    let mut active: images::ActiveModel = existing.into();
    active.name = Set(payload.name);
    active.docker_images = Set(payload.docker_images);
    active.description = Set(payload.description);
    active.startup_command = Set(payload.startup_command);
    active.stop_command = Set(payload.stop_command);
    active.requires_port = Set(payload.requires_port);
    active.log_config = Set(payload.log_config);
    active.config_files = Set(payload.config_files);
    active.start_config = Set(payload.start_config);
    active.install_script = Set(payload.install_script);
    active.install_container = Set(payload.install_container);
    active.install_entrypoint = Set(payload.install_entrypoint);
    active.variables = Set(payload.variables);

    let _ = active.update(&state.db).await;

    Redirect::to("/runtimes")
}

pub async fn delete_image_handler(
    State(state): State<AppState>,
    axum::extract::Path((_runtime_id, image_id)): axum::extract::Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    let _ = Images::delete_by_id(image_id).exec(&state.db).await;

    let mut headers = axum::http::HeaderMap::new();
    headers.insert("HX-Redirect", "/runtimes".parse().unwrap());
    (headers, "Deleted")
}

// Egg Import
#[derive(serde::Deserialize)]
pub struct ImportEggRequest {
    pub egg_file: String, // Or JSON content?
    // For now we might just support JSON paste in a textarea or file upload
}

// Simplified handle for now
pub async fn import_egg_handler(
    State(_state): State<AppState>,
    axum::extract::Path(_id): axum::extract::Path<Uuid>,
) -> impl IntoResponse {
    // Placeholder
    Redirect::to("/runtimes")
}

use crate::http::handlers::HtmlTemplate;
use crate::state::AppState;
use crate::entities::users::Entity as Users;
use crate::entities::users; // For ActiveModel
use askama::Template;
use axum::{
    extract::{Form, Path, State},
    response::{IntoResponse, Redirect},
};
use sea_orm::{EntityTrait, Set, ActiveModelTrait, QueryOrder};
use serde::Deserialize;
use uuid::Uuid;
use crate::services::auth::hash_password;

#[derive(Template)]
#[template(path = "users.html")]
struct UsersTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String,
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    users: Vec<users::Model>,
}

#[derive(Template)]
#[template(path = "user_create.html")]
struct CreateUserTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String,
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "user_edit.html")]
struct EditUserTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String,
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    user: users::Model,
    error: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub role: String, // "admin" or "user"
}

#[derive(Deserialize)]
pub struct UpdateUserRequest {
    pub username: String,
    pub email: String,
    pub password: Option<String>,
    pub role: String,
}

pub async fn list_users(State(state): State<AppState>) -> impl IntoResponse {
    let start = std::time::Instant::now();
    
    let users = Users::find()
        .order_by_asc(users::Column::Username)
        .all(&state.db)
        .await
        .unwrap_or_default();

    let panel_name = state.panel_name.read().await.clone();
    let execution_time = start.elapsed().as_secs_f64() * 1000.0;

    HtmlTemplate(UsersTemplate {
        panel_name,
        panel_font: state.panel_font.read().await.clone(),
        panel_font_url: state.panel_font_url.read().await.clone(),
        panel_version: env!("CARGO_PKG_VERSION").to_string(),   
        execution_time,
        active_tab: "users".to_string(),
        users,
    })
}

pub async fn create_user_page(State(state): State<AppState>) -> impl IntoResponse {
    let start = std::time::Instant::now();
    HtmlTemplate(CreateUserTemplate {
        panel_name: state.panel_name.read().await.clone(),
        panel_font: state.panel_font.read().await.clone(),
        panel_font_url: state.panel_font_url.read().await.clone(),
        panel_version: env!("CARGO_PKG_VERSION").to_string(),
        execution_time: start.elapsed().as_secs_f64() * 1000.0,
        active_tab: "users".to_string(),
        error: None,
    })
}

pub async fn create_user(
    State(state): State<AppState>,
    Form(form): Form<CreateUserRequest>,
) -> impl IntoResponse {
    let hashed = match hash_password(&form.password) {
        Ok(h) => h,
        Err(_) => return Redirect::to("/users/new?error=hash_failed").into_response(),
    };

    let user = users::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set(form.username),
        email: Set(form.email),
        password_hash: Set(hashed),
        role: Set(form.role),
        permissions: Set(None),
        created_at: Set(chrono::Utc::now()),
    };

    if let Err(e) = Users::insert(user).exec(&state.db).await {
        eprintln!("Error creating user: {}", e);
        return Redirect::to("/users/new?error=db_error").into_response();
    }

    Redirect::to("/users").into_response()
}

pub async fn edit_user_page(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let user = match Users::find_by_id(id).one(&state.db).await {
         Ok(Some(u)) => u,
         _ => return Redirect::to("/users").into_response(),
    };

    HtmlTemplate(EditUserTemplate {
        panel_name: state.panel_name.read().await.clone(),
        panel_font: state.panel_font.read().await.clone(),
        panel_font_url: state.panel_font_url.read().await.clone(),
        panel_version: env!("CARGO_PKG_VERSION").to_string(),
        execution_time: start.elapsed().as_secs_f64() * 1000.0,
        active_tab: "users".to_string(),
        user,
        error: None,
    }).into_response()
}

pub async fn update_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Form(form): Form<UpdateUserRequest>,
) -> impl IntoResponse {
    let user = match Users::find_by_id(id).one(&state.db).await {
         Ok(Some(u)) => u,
         _ => return Redirect::to("/users").into_response(),
    };
    
    let mut active: users::ActiveModel = user.into();
    active.username = Set(form.username);
    active.email = Set(form.email);
    active.role = Set(form.role);

    if let Some(pw) = form.password.filter(|p| !p.is_empty()) {
          if let Ok(h) = hash_password(&pw) {
               active.password_hash = Set(h);
          }
    }

    if let Err(e) = active.update(&state.db).await {
         eprintln!("Error updating user: {}", e);
         return Redirect::to(&format!("/users/{}/edit?error=update_failed", id)).into_response();
    }

    Redirect::to("/users").into_response()
}

pub async fn delete_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let _ = Users::delete_by_id(id).exec(&state.db).await;
    Redirect::to("/users").into_response()
}

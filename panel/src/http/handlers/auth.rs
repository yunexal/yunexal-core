use axum::{
    extract::{State, Path, Form},
    http::{StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Router,
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use crate::{state::AppState, http::handlers::HtmlTemplate, services::auth::verify_password};
use askama::Template;
use chrono::{Utc, Duration};
use rand::Rng;
use serde::Deserialize;
use uuid::Uuid;
use sea_orm::{EntityTrait, QueryFilter, ColumnTrait, ActiveModelTrait, Set, IntoActiveModel, ActiveValue};
use crate::entities::{users, sessions, nodes};

pub fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/login", get(login_page).post(login_handler))
        .route("/logout", post(logout_handler))
}

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    pub error: Option<String>,
    pub panel_font: String,
    pub panel_font_url: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    email: String,
    password: String,
}

pub async fn login_page(State(state): State<AppState>) -> impl IntoResponse {
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();
    
    HtmlTemplate(LoginTemplate { 
        error: None,
        panel_font,
        panel_font_url,
    })
}

pub async fn login_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(payload): Form<LoginRequest>,
) -> Response {
    // SeaORM find user
    let user_opt: Option<users::Model> = users::Entity::find()
        .filter(users::Column::Email.eq(&payload.email))
        .one(&state.db)
        .await
        .unwrap_or(None);

    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();

    if let Some(user) = user_opt {
        if verify_password(&payload.password, &user.password_hash) {
            // Create session
            let session_id = Uuid::new_v4();
            let expires_at = Utc::now() + Duration::days(7);
            
            let new_session = sessions::ActiveModel {
                id: ActiveValue::Set(session_id),
                user_id: ActiveValue::Set(user.id),
                expires_at: ActiveValue::Set(expires_at),
            };

            if new_session.insert(&state.db).await.is_ok() {
                let cookie = Cookie::build(("session_id", session_id.to_string()))
                    .path("/")
                    .http_only(true)
                    .secure(false) // Set to true in prod
                    .max_age(time::Duration::days(7));

                return (jar.add(cookie), Redirect::to("/")).into_response();
            }
        }
    }

    HtmlTemplate(LoginTemplate { 
        error: Some("Invalid email or password".into()),
        panel_font,
        panel_font_url,
    }).into_response()
}

pub async fn logout_handler(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    if let Some(session_cookie) = jar.get("session_id") {
         let session_str = session_cookie.value();
         if let Ok(sess_uuid) = Uuid::parse_str(session_str) {
             let _ = sessions::Entity::delete_by_id(sess_uuid)
                .exec(&state.db)
                .await;
         }
    }
    
    (jar.remove(Cookie::from("session_id")), Redirect::to("/auth/login"))
}

pub async fn rotate_token_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> StatusCode {
    let uid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return StatusCode::NOT_FOUND,
    };

    let node_opt = nodes::Entity::find_by_id(uid)
        .one(&state.db)
        .await
        .unwrap_or(None);

    if let Some(node) = node_opt {
        // use rand::Rng; // Already imported
        let new_token: String = rand::rng()
            .sample_iter(&rand::distr::Alphanumeric) 
            .take(32)
            .map(char::from)
            .collect();

        // Use Cache Service
        let key = format!("node:{}:pending_token", id);
        state.cache.set(&key, &new_token, 60).await;

        let url = format!("http://{}:{}/update-token", node.ip, node.port);
        let payload = serde_json::json!({ "token": new_token });

        let resp = state.http_client.post(&url)
            .header("Authorization", format!("Bearer {}", node.token))
            .json(&payload)
            .send()
            .await;

        match resp {
            Ok(res) if res.status().is_success() => {
                let mut active: nodes::ActiveModel = node.clone().into_active_model();
                active.token = Set(new_token);
                let _ = active.update(&state.db).await;
                
                return StatusCode::OK;
            }
            _ => return StatusCode::INTERNAL_SERVER_ERROR
        }
    }
    StatusCode::NOT_FOUND
}

use axum::{
    middleware::Next,
    extract::Request,
};

pub async fn auth_middleware(
    State(state): State<AppState>,
    jar: CookieJar,
    request: Request,
    next: Next,
) -> Result<Response, Redirect> {
    let session_cookie = jar.get("session_id");
    if let Some(cookie) = session_cookie {
        if let Ok(session_id) = Uuid::parse_str(cookie.value()) {
            let sess = sessions::Entity::find_by_id(session_id)
                .one(&state.db)
                .await
                .unwrap_or(None);
                
            if let Some(s) = sess {
                 if s.expires_at > chrono::Utc::now() {
                     return Ok(next.run(request).await);
                 }
            }
        }
    }
    
    // Check if path is safely ignorable?
    // Handlers logic in main.rs applies middleware ONLY to protected routes.
    // If we are here, we FAILED validation.
    
    Err(Redirect::to("/auth/login"))
}

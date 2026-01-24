use axum::{
    Router,
    routing::{delete, get, post},
};
use redis::Client as RedisClient;
use sea_orm::{Database, DatabaseConnection, Statement, ConnectionTrait, DbBackend};
use std::net::SocketAddr;
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use panel::http;
use panel::services;
use panel::state;

use http::handlers::{
    allocations::{
        allocations_page_handler, create_allocations_handler, delete_allocations_handler,
    },
    api::heartbeat_handler,
    auth::{rotate_token_handler, auth_routes},
    dashboard::nodes_page_handler,
    logs::logs_handler,
    nodes::{
        create_node_handler, create_node_page_handler, delete_node_handler, edit_node_page_handler,
        setup_node_page_handler, trigger_node_update, update_node_handler, download_node_agent,
    },
    overview::{overview_handler, overview_stats_handler},
    runtimes::{
        create_image_handler, create_image_page_handler, create_runtime_handler,
        create_runtime_page_handler, delete_image_handler, delete_runtime_handler,
        edit_image_page_handler, edit_runtime_page_handler, import_egg_handler,
        reorder_runtimes_handler, runtimes_page_handler, update_image_handler,
        update_runtime_handler,
    },
    scripts::{install_script_handler, uninstall_script_handler},
    servers::{
        create_server_handler, create_server_page_handler, delete_server_handler,
        edit_server_page_handler, manage_server_page_handler, servers_page_handler,
        update_server_handler,
        start_server_handler, stop_server_handler, restart_server_handler
    },

    users::{
        list_users, create_user, create_user_page, edit_user_page, update_user, delete_user
    },
};
use state::AppState;
use panel::http::handlers::auth::auth_middleware;

/// Runs migrations using SeaORM connection but executing raw SQL for legacy compatibility
/// Ideally we should move to SeaORM migrations (generating migration files).
async fn run_migrations(db: &DatabaseConnection) {
    let backend = db.get_database_backend();
    let is_sqlite = backend == DbBackend::Sqlite;

    if is_sqlite {
        db.execute(Statement::from_string(backend, "PRAGMA foreign_keys = ON;")).await.ok();
    }

    let uuid_type = if is_sqlite { "TEXT" } else { "UUID" };
    let _timestamp_type = if is_sqlite { "DATETIME" } else { "TIMESTAMPTZ" };
    // let default_now = "CURRENT_TIMESTAMP"; 

    // Nodes Table
    let sql = format!(
        r#"
        CREATE TABLE IF NOT EXISTS nodes (
            id {} PRIMARY KEY,
            name TEXT NOT NULL,
            ip TEXT NOT NULL,
            port INTEGER NOT NULL,
            token TEXT NOT NULL
        )
    "#, uuid_type);
    db.execute(Statement::from_string(backend, sql)).await.ok();

    // Node Migrations
    let migrations = vec![
        "ALTER TABLE nodes ADD COLUMN IF NOT EXISTS token TEXT",
        "ALTER TABLE nodes ADD COLUMN IF NOT EXISTS sftp_port INTEGER DEFAULT 2022",
        "ALTER TABLE nodes ADD COLUMN IF NOT EXISTS ram_limit INTEGER DEFAULT 0",
        "ALTER TABLE nodes ADD COLUMN IF NOT EXISTS disk_limit INTEGER DEFAULT 0",
        "ALTER TABLE nodes ADD COLUMN IF NOT EXISTS cpu_limit INTEGER DEFAULT 0",
        "ALTER TABLE nodes ADD COLUMN IF NOT EXISTS version TEXT DEFAULT ''",
    ];
    for m in migrations {
        db.execute(Statement::from_string(backend, m)).await.ok();
    }

    // Allocations
    let sql = format!(
        r#"
        CREATE TABLE IF NOT EXISTS allocations (
            id {0} PRIMARY KEY,
            node_id {0} NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
            ip TEXT NOT NULL,
            port INTEGER NOT NULL,
            server_id {0},
            UNIQUE(node_id, ip, port)
        )
    "#, uuid_type);
    db.execute(Statement::from_string(backend, sql)).await.ok();

    // Runtimes
    let sql = format!(
        r#"
        CREATE TABLE IF NOT EXISTS runtimes (
            id {} PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            color TEXT DEFAULT '#007bff'
        )
    "#, uuid_type);
    db.execute(Statement::from_string(backend, sql)).await.ok();

    // Images
    let sql = format!(
        r#"
        CREATE TABLE IF NOT EXISTS images (
            id {0} PRIMARY KEY,
            runtime_id {0} NOT NULL REFERENCES runtimes(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            docker_images TEXT NOT NULL,
            description TEXT,
            stop_command TEXT NOT NULL DEFAULT 'stop',
            startup_command TEXT NOT NULL DEFAULT '',
            log_config TEXT NOT NULL DEFAULT '{{}}',
            config_files TEXT NOT NULL DEFAULT '[]',
            start_config TEXT NOT NULL DEFAULT '{{}}',
            requires_port BOOLEAN NOT NULL DEFAULT TRUE
        )
    "#, uuid_type);
    db.execute(Statement::from_string(backend, sql)).await.ok();
    
    // Image Migrations 
    let image_migrations = vec![
        "ALTER TABLE images ADD COLUMN IF NOT EXISTS docker_images TEXT DEFAULT ''",
        "ALTER TABLE images DROP COLUMN IF EXISTS docker_image",
        "ALTER TABLE images ADD COLUMN IF NOT EXISTS stop_command TEXT DEFAULT 'stop'",
        "ALTER TABLE images ADD COLUMN IF NOT EXISTS startup_command TEXT DEFAULT ''",
        "ALTER TABLE images ADD COLUMN IF NOT EXISTS log_config TEXT DEFAULT '{}'",
        "ALTER TABLE images ADD COLUMN IF NOT EXISTS config_files TEXT DEFAULT '[]'",
        "ALTER TABLE images ADD COLUMN IF NOT EXISTS start_config TEXT DEFAULT '{}'",
        "ALTER TABLE images ADD COLUMN IF NOT EXISTS requires_port BOOLEAN DEFAULT TRUE",
        "ALTER TABLE images ADD COLUMN IF NOT EXISTS install_script TEXT DEFAULT ''",
        "ALTER TABLE images ADD COLUMN IF NOT EXISTS install_container TEXT DEFAULT ''",
        "ALTER TABLE images ADD COLUMN IF NOT EXISTS install_entrypoint TEXT DEFAULT 'bash'",
        "ALTER TABLE images ADD COLUMN IF NOT EXISTS variables TEXT DEFAULT '[]'",
    ];
    
    // Attempt data migration for docker_image -> docker_images
    db.execute(Statement::from_string(backend, "UPDATE images SET docker_images = docker_image WHERE (docker_images IS NULL OR docker_images = '') AND docker_image IS NOT NULL")).await.ok();
    
    for m in image_migrations {
         db.execute(Statement::from_string(backend, m)).await.ok();
    }
    
    // Runtimes Extra
    db.execute(Statement::from_string(backend, "ALTER TABLE runtimes ADD COLUMN IF NOT EXISTS color TEXT DEFAULT '#007bff'")).await.ok();
    db.execute(Statement::from_string(backend, "ALTER TABLE runtimes ADD COLUMN IF NOT EXISTS sort_order INTEGER DEFAULT 0")).await.ok();

     // Users
    let sql = format!(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id {} PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            email TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'user',
            permissions TEXT,
            created_at TEXT NOT NULL
        )
    "#, uuid_type);
    db.execute(Statement::from_string(backend, sql)).await.ok();
    
    // Sessions
    let sql = format!(
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            id {} PRIMARY KEY,
            user_id {} NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            token TEXT NOT NULL UNIQUE,
            expires_at TEXT NOT NULL
        )
    "#, uuid_type, uuid_type);
    db.execute(Statement::from_string(backend, sql)).await.ok();

    // Servers
    let sql = format!(
        r#"
        CREATE TABLE IF NOT EXISTS servers (
            id {0} PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            owner_id {0} REFERENCES users(id) ON DELETE SET NULL,
            node_id {0} NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
            allocation_id {0} REFERENCES allocations(id) ON DELETE SET NULL,
            image_id {0} REFERENCES images(id) ON DELETE RESTRICT,
            
            cpu_limit INTEGER NOT NULL DEFAULT 0,
            ram_limit INTEGER NOT NULL DEFAULT 0,
            disk_limit INTEGER NOT NULL DEFAULT 0,
            swap_limit INTEGER NOT NULL DEFAULT 0,
            backup_limit INTEGER NOT NULL DEFAULT 0,
            
            io_weight INTEGER NOT NULL DEFAULT 500,
            oom_killer BOOLEAN NOT NULL DEFAULT TRUE,
            
            docker_image TEXT NOT NULL,
            startup_command TEXT NOT NULL,
            
            cpu_pinning TEXT,
            
            status TEXT NOT NULL DEFAULT 'installing',
            created_at TEXT NOT NULL
        )
    "#, uuid_type);
    db.execute(Statement::from_string(backend, sql)).await.ok();
    
    // Migration: allocations can be optional for servers
    // Only run on Postgres as SQLite doesn't support ALTER COLUMN
    if !is_sqlite {
         db.execute(Statement::from_string(backend, "ALTER TABLE servers ALTER COLUMN allocation_id DROP NOT NULL")).await.ok();
    }
}

async fn create_default_admin(db: &DatabaseConnection) {
    let backend = db.get_database_backend();
    let is_sqlite = backend == DbBackend::Sqlite;
    
    // Check if we need to seed Admin user
    // We can use SeaORM entity here but since we haven't refactored handlers yet, let's keep it consistent
    // Actually no, let's use SeaORM Entity if possible, or raw SQL.
    use sea_orm::prelude::*;
    // query_one behaves differently with raw sql across backends sometimes, but `query_one` expects a Statement
    let res = db.query_one(Statement::from_string(backend, "SELECT COUNT(*) as count FROM users")).await.ok().flatten();
    
    // Fix: query_one returns generic QueryResult. Accessing by name "count".
    let user_count: i64 = if let Some(row) = res {
        // try_get from sea_orm::QueryResult uses (pos, name)
        row.try_get("", "count").unwrap_or(0)
    } else {
        0
    };

    if user_count == 0 {
        tracing::info!("No users found. Creating default 'admin' user...");
        let admin_id = uuid::Uuid::new_v4();
        let password = "admin";
        let hash = panel::services::auth::hash_password(password).expect("Failed to hash password");

        let cast_uuid = if is_sqlite { "" } else { "::uuid" };
        
        // Note: For PG, we need manual quoting for strings in raw SQL if binding isn't used. 
        // SeaORM `Statement` does not support binding in `from_string` directly without `Statement::from_sql_and_values`.
        // So constructing string is dangerous but here inputs are safe hardcoded.
        // except `hash` which is output of bcrypt.
        // Let's use `Statement::from_sql_and_values`!
        
        let sql = format!(
            "INSERT INTO users (id, username, email, password_hash, role, permissions, created_at) VALUES ($1{}, $2, $3, $4, $5, $6, $7)",
            cast_uuid
        );
        
        use sea_orm::{Value};
        let stmt = Statement::from_sql_and_values(backend, &sql, vec![
            Value::from(admin_id.to_string()),
            Value::from("admin"),
            Value::from("admin@example.com"),
            Value::from(hash),
            Value::from("admin"),
            Value::from("{}"),
            Value::from(chrono::Utc::now().to_rfc3339()),
        ]);

        db.execute(stmt).await.expect("Failed to create admin user");

        tracing::info!(
            "Default admin user created. Username: 'admin', Password: 'admin'. PLEASE CHANGE THIS IMMEDIATELY!"
        );
    }
}

#[tokio::main]
async fn main() {
    // Load environment variables from .env file
    dotenv::dotenv().ok();

    // Initialize Logging
    let file_appender = tracing_appender::rolling::daily("logs", "panel.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false),
        )
        .init();

    tracing::info!("Starting Yunexal Panel...");

    // Initialize Database Connection
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost/yunexal".to_string());
        
    let db = Database::connect(&db_url).await.expect("Failed to connect to Database");
    let backend = db.get_database_backend();
    let is_sqlite = backend == DbBackend::Sqlite;

    // Run "migrations"
    run_migrations(&db).await;
    create_default_admin(&db).await;

    // Initialize Redis
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/".to_string());
    // Since we removed redis types from main imports, we need to verify import
    // "use redis::Client as RedisClient" is at top
    let _redis = RedisClient::open(redis_url).expect("Invalid Redis URL");
    
    // Services
    let redis_url_opt = std::env::var("REDIS_URL").ok();
    let cache_service = services::cache::CacheService::new(redis_url_opt).await;

    let state = AppState {
        db,
        cache: cache_service,
        http_client: reqwest::Client::new(),
        panel_name: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::env::var("PANEL_NAME").unwrap_or_else(|_| "Yunexal Panel".to_string()),
        )),
        panel_font: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::env::var("PANEL_FONT").unwrap_or_else(|_| "Google Sans Flex".to_string()),
        )),
        panel_font_url: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::env::var("PANEL_FONT_URL").unwrap_or_default(),
        )),
        nodes_cache: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        heartbeats_cache: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::HashMap::new(),
        )),
        is_sqlite,
    };

    // Build our application with a route
    let protected_routes = Router::new()
        .route("/", get(overview_handler))
        .route("/overview/stats", get(overview_stats_handler))
        .route(
            "/settings/update",
            post(http::handlers::overview::update_settings_handler),
        )
        .route("/nodes", get(nodes_page_handler).post(create_node_handler))
        .route("/servers", get(servers_page_handler).post(create_server_handler))
        .route("/servers/new", get(create_server_page_handler))
        .route("/servers/{id}/manage", get(manage_server_page_handler))
        .route("/servers/{id}/start", post(start_server_handler))
        .route("/servers/{id}/stop", post(stop_server_handler))
        .route("/servers/{id}/restart", post(restart_server_handler))
        .route("/servers/{id}/edit", get(edit_server_page_handler))
        .route("/servers/{id}/update", post(update_server_handler))
        .route("/servers/{id}/delete", post(delete_server_handler))
        .route("/users", get(list_users).post(create_user))
        .route("/users/new", get(create_user_page))
        .route("/users/{id}/edit", get(edit_user_page).post(update_user))
        .route("/users/{id}/delete", post(delete_user))
        .route(
            "/runtimes",
            get(runtimes_page_handler).post(create_runtime_handler),
        )
        .route("/runtimes/reorder", post(reorder_runtimes_handler))
        .route("/runtimes/new", get(create_runtime_page_handler))
        .route("/runtimes/{id}/edit", get(edit_runtime_page_handler))
        .route("/runtimes/{id}/update", post(update_runtime_handler))
        .route("/runtimes/{id}/images/new", get(create_image_page_handler))
        .route("/runtimes/{id}/images", post(create_image_handler))
        .route("/runtimes/{id}/images/import", post(import_egg_handler))
        .route(
            "/runtimes/{runtime_id}/images/{image_id}/edit",
            get(edit_image_page_handler),
        )
        .route(
            "/runtimes/{runtime_id}/images/{image_id}/update",
            post(update_image_handler),
        )
        .route(
            "/runtimes/{runtime_id}/images/{image_id}",
            delete(delete_image_handler),
        )
        .route("/runtimes/{id}", delete(delete_runtime_handler))
        .route("/logs", get(logs_handler))
        .route("/nodes/new", get(create_node_page_handler))
        .route("/nodes/{id}/setup", get(setup_node_page_handler))
        .route("/nodes/{id}/edit", get(edit_node_page_handler))
        .route(
            "/nodes/{id}/allocations",
            get(allocations_page_handler).post(create_allocations_handler),
        )
        .route(
            "/nodes/{id}/allocations/delete",
            post(delete_allocations_handler),
        )
        .route("/nodes/{id}/update", post(update_node_handler))
        .route("/nodes/{id}/trigger-update", post(trigger_node_update))
        .route("/nodes/{id}/rotate-token", post(rotate_token_handler))
        .route("/nodes/{id}", delete(delete_node_handler))
        .layer(axum::middleware::from_fn_with_state(state.clone(), auth_middleware));

    let public_routes = Router::new()
        .route("/nodes/{id}/heartbeat", post(heartbeat_handler))
        .route("/install/{id}", get(install_script_handler))
        .route("/uninstall/{id}", get(uninstall_script_handler))
        .nest("/auth", auth_routes())
        .nest_service("/assets", ServeDir::new("public/assets"))
        .route("/downloads/yunexal-node", get(download_node_agent)); // Secured download

    let app = Router::new()
        .merge(protected_routes)
        .merge(public_routes)
        .with_state(state);

    // Run it
    // Bind to 0.0.0.0 to allow external access
    let port = std::env::var("PANEL_PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("Panel listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();
}

#![allow(deprecated)]
use bollard::Docker;
use axum::{
    routing::{delete, get, post},
    Router,
    middleware,
};
use std::net::SocketAddr;
use std::fs;

mod models;
mod state;
mod handlers;
mod tasks;
mod grpc;
mod services;

use models::NodeConfig;
use state::NodeState;
use handlers::{
    auth::{auth_middleware, update_token_handler},
    docker::{console_handler, create_container, delete_container, list_containers, start_container, stop_container, restart_container},
    health::health_check,
    update::self_update_handler,
};
use tasks::start_heartbeat_task;
use grpc::MyNodeService;
use grpc::node_proto::node_service_server::NodeServiceServer;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    // Load env if .env file exists (optional fallback)
    dotenv::dotenv().ok(); 

    println!("Starting Yunexal Node Agent...");
    println!("Current Working Directory: {:?}", std::env::current_dir().unwrap_or_default());

    // Try to load config.yml
    let config_path = "config.yml";
    let config = match fs::read_to_string(config_path) {
        Ok(content) => {
             println!("Found config.yml, parsing...");
             match serde_yaml::from_str::<NodeConfig>(&content) {
                 Ok(cfg) => Some(cfg),
                 Err(e) => {
                     eprintln!("Failed to parse config.yml: {}", e);
                     eprintln!("Content was: \n{}", content);
                     None
                 }
             }
        },
        Err(e) => {
            println!("config.yml not found or unreadable: {}", e);
            None
        }
    };

    let (token, node_id, panel_url, port, ram_limit, disk_limit) = if let Some(mut cfg) = config {
        println!("Loaded configuration from config.yml");
        
        // ... (existing auto-config code)
        let mut sys = sysinfo::System::new_all();
        sys.refresh_all();
        
        // Auto-configure RAM Limit (95%)
        if cfg.ram_limit == 0 {
             let total_ram_mb = sys.total_memory() / 1024 / 1024;
             cfg.ram_limit = (total_ram_mb as f64 * 0.95) as u64;
             println!("Auto-configured RAM limit to {:.2} GB (95% of {:.2} GB)", cfg.ram_limit as f64 / 1024.0, total_ram_mb as f64 / 1024.0);
        }

        // Auto-configure Disk Limit (95%)
        if cfg.disk_limit == 0 {
            let disks = sysinfo::Disks::new_with_refreshed_list();
            // Simple heuristic: Find largest available space or root
            let mut total_space_mb = 0;
            for disk in &disks {
                 if disk.mount_point() == std::path::Path::new("/") {
                     total_space_mb = disk.total_space() / 1024 / 1024;
                     break;
                 }
            }
            if total_space_mb == 0 && !disks.is_empty() {
                total_space_mb = disks[0].total_space() / 1024 / 1024;
            }

            cfg.disk_limit = (total_space_mb as f64 * 0.95) as u64;
            println!("Auto-configured Disk limit to {:.2} GB (95% of {:.2} GB)", cfg.disk_limit as f64 / 1024.0, total_space_mb as f64 / 1024.0);
        }

        (cfg.token, cfg.node_id, cfg.panel_url, cfg.port, cfg.ram_limit, cfg.disk_limit)
    } else {
        println!("ERROR: config.yml not loaded and fallback ENV vars are missing.");
        println!("Please ensure config.yml exists in {:?} and is valid YAML.", std::env::current_dir().unwrap_or_default());
        // Force exit with specific error to avoid panic 101
        std::process::exit(1); 
    };

    println!("Node ID: {}", node_id);
    println!("Panel URL: {}", panel_url);
    println!("Port: {}", port);

    // Connect to Docker
    let docker = Docker::connect_with_local_defaults()?;
    
    // Verify connection
    let version = docker.version().await?;
    println!("Connected to Docker daemon version: {:?}", version.version.unwrap_or_default());

    let state = NodeState { 
        docker,
        token: std::sync::Arc::new(tokio::sync::RwLock::new(token)),
        node_id,
        panel_url,
        port,
        ram_limit,
        disk_limit,
    };

    // Build our application with routes
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/containers", get(list_containers))
        .route("/containers", post(create_container))
        .route("/containers/{uuid}", delete(delete_container))
        .route("/containers/{uuid}/start", post(start_container))
        .route("/containers/{uuid}/stop", post(stop_container))
        .route("/containers/{uuid}/restart", post(restart_container))
        .route("/containers/{uuid}/console", get(console_handler))
        .route("/update-token", post(update_token_handler))
        .route("/self-update", post(self_update_handler))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .with_state(state.clone());

    // Run it on configured port
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("Node Agent listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    // Start heartbeat task
    tokio::spawn(start_heartbeat_task(state.clone()));

    // Start gRPC Server
    let grpc_port = port + 1;
    let grpc_addr = format!("0.0.0.0:{}", grpc_port).parse().unwrap();
    let node_service = MyNodeService { docker: state.docker.clone() };

    tokio::spawn(async move {
        println!("gRPC Server listening on {}", grpc_addr);
        Server::builder()
            .add_service(NodeServiceServer::new(node_service))
            .serve(grpc_addr)
            .await
            .expect("gRPC server failed");
    });

    axum::serve(listener, app).await.unwrap();

    Ok(())
}


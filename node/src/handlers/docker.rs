use crate::{models::CreateContainerRequest, state::NodeState};
use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, Json, Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use bollard::query_parameters::{
    AttachContainerOptions, CreateContainerOptions, ListContainersOptions,
    RemoveContainerOptions, StartContainerOptions, StopContainerOptions, RestartContainerOptions, CreateImageOptions,
};
use bollard::service::{ContainerCreateBody as DockerConfig, HostConfig, PortBinding};
use futures_util::{StreamExt, SinkExt};
use std::collections::HashMap;
use tokio::io::AsyncWriteExt; // For writing to container input

fn is_port_free(port: u16) -> bool {
    std::net::TcpListener::bind(("0.0.0.0", port)).is_ok()
}

fn get_port_occupier(port: u16) -> String {
    use std::process::Command;
    // Try lsof first
    if let Ok(output) = Command::new("lsof").args(&["-i", &format!(":{}", port), "-t"]).output() {
        if !output.stdout.is_empty() {
            let pid = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // Try get process name
            if let Ok(ps_out) = Command::new("ps").args(&["-p", &pid, "-o", "comm="]).output() {
                let comm = String::from_utf8_lossy(&ps_out.stdout).trim().to_string();
                return format!("Process {} (PID: {})", comm, pid);
            }
            return format!("PID {}", pid);
        }
    }
    // Try ss
    if let Ok(output) = Command::new("ss").args(&["-lptn", &format!("sport = :{}", port)]).output() {
         let s = String::from_utf8_lossy(&output.stdout).to_string();
         if let Some(line) = s.lines().nth(1) {
             return line.to_string();
         }
    }
    "Unknown".to_string()
}

pub async fn list_containers(State(state): State<NodeState>) -> Json<Vec<String>> {
    let mut filters: HashMap<String, Vec<String>> = HashMap::new();
    //DO NOT CHANGE THIS LABEL ANYWAY!
    //Why? Because it's used to identify containers created by Node.
    //Avoid conflicts with other containers.
    filters.insert(
        "label".to_string(),
        vec!["yunexal.managed=true".to_string()],
    );

    let options = Some(ListContainersOptions {
        all: true,
        filters: Some(filters),
        ..Default::default()
    });

    match state.docker.list_containers(options).await {
        Ok(containers) => {
            let names: Vec<String> = containers
                .into_iter()
                .map(|c| {
                    let name = c
                        .names
                        .unwrap_or_default()
                        .first()
                        .map(|s| s.to_string())
                        .unwrap_or("unknown".to_string());
                    let state = c
                        .state
                        .map(|s| format!("{:?}", s))
                        .unwrap_or_else(|| "unknown".to_string());
                    format!("{} [{}]", name, state)
                })
                .collect();
            Json(names)
        }
        Err(_) => Json(vec!["Error listing containers".to_string()]),
    }
}

pub async fn create_container(
    State(state): State<NodeState>,
    Json(payload): Json<CreateContainerRequest>,
) -> Result<Json<String>, StatusCode> {
    let mut resolved_ports_map: HashMap<String, String> = HashMap::new();

    // Check availability and Auto-Assign Ports
    for (container_port, host_port_str) in &payload.ports {
        let original_port = host_port_str.parse::<u16>().unwrap_or(0);
        let mut final_port = original_port;

        if original_port > 0 {
             if !is_port_free(final_port) {
                 println!("Port {} is occupied. Iterating to find a free port...", final_port);
                 let mut found = false;
                 // Try next 50 ports suitable for games
                 for i in 1..=50 { 
                     let candidate = original_port + i;
                     if is_port_free(candidate) {
                         println!("Found free port: {} (Original was {})", candidate, original_port);
                         final_port = candidate;
                         found = true;
                         break;
                     }
                 }
                 
                 if !found {
                      let occupier = get_port_occupier(original_port);
                      eprintln!("Failed to allocate port. Original {} occupied by: {}", original_port, occupier);
                      eprintln!("Tried range {}-{} with no luck.", original_port, original_port + 50);
                      return Err(StatusCode::CONFLICT);
                 }
             }
        }
        resolved_ports_map.insert(container_port.clone(), final_port.to_string());
    }

    let image_name = payload.image.clone();
    // Pull the image first
    println!("Pulling image: {}", image_name);
    let create_image_options = Some(CreateImageOptions {
        from_image: Some(image_name.clone()),
        ..Default::default()
    });

    // Use a stream processing to wait for the pull to complete
    let mut stream = state.docker.create_image(create_image_options, None, None);
    while let Some(result) = stream.next().await {
        if let Err(e) = result {
             eprintln!("Error pulling image {}: {}", image_name, e);
             return Err(StatusCode::BAD_REQUEST);
        }
    }
    println!("Image pulled successfully: {}", image_name);

    let container_name = format!("yunexal-{}", payload.uuid);

    println!("Creating container: {}", container_name); // Log creation attempt

    let options = Some(CreateContainerOptions {
        name: Some(container_name.clone()),
        ..Default::default()
    });

    let mut labels = HashMap::new();
    labels.insert("yunexal.managed".to_string(), "true".to_string());
    labels.insert("yunexal.server_id".to_string(), payload.uuid.clone());

    // Port Bindings using RESOLVED ports
    let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
    for (container_port, host_port) in resolved_ports_map {
        let binding = vec![PortBinding {
            host_ip: Some("0.0.0.0".to_string()),
            host_port: Some(host_port),
        }];
        port_bindings.insert(container_port, Some(binding));
    }

    // Host Config (Limits)
    let host_config = HostConfig {
        memory: Some(payload.memory_limit * 1024 * 1024), // MB to Bytes
        memory_swap: Some(payload.swap_limit * 1024 * 1024),
        nano_cpus: Some(payload.cpu_limit * 10_000_000), 
        blkio_weight: Some(payload.io_weight),
        port_bindings: Some(port_bindings),
        ..Default::default()
    };

    // Env Vars
    let env: Vec<String> = payload
        .environment
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();

    let config = DockerConfig {
        image: Some(payload.image),
        labels: Some(labels),
        env: Some(env),
        // Wrap command in shell to ensure variable expansion and simple parsing works
        cmd: Some(vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            payload.startup_command,
        ]),
        host_config: Some(host_config),
        tty: Some(true), // Enable TTY for console access
        open_stdin: Some(true), // Keep stdin open
        attach_stdin: Some(true),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        ..Default::default()
    };

    match state.docker.create_container(options, config).await {
        Ok(res) => {
            // Start the container
            if let Err(e) = state
                .docker
                .start_container(&res.id, None::<StartContainerOptions>)
                .await
            {
                eprintln!("Failed to start container: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
            Ok(Json(res.id))
        }
        Err(e) => {
            eprintln!("Failed to create container: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}


pub async fn start_container(
    State(state): State<NodeState>,
    Path(uuid): Path<String>,
) -> Result<Json<String>, StatusCode> {
    let container_name = format!("yunexal-{}", uuid);
    match state.docker.start_container(&container_name, None::<StartContainerOptions>).await {
        Ok(_) => Ok(Json("started".to_string())),
        Err(e) => {
            eprintln!("Failed to start container: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn stop_container(
    State(state): State<NodeState>,
    Path(uuid): Path<String>,
) -> Result<Json<String>, StatusCode> {
    let container_name = format!("yunexal-{}", uuid);
    match state.docker.stop_container(&container_name, None::<StopContainerOptions>).await {
        Ok(_) => Ok(Json("stopped".to_string())),
        Err(e) => {
            eprintln!("Failed to stop container: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn restart_container(
    State(state): State<NodeState>,
    Path(uuid): Path<String>,
) -> Result<Json<String>, StatusCode> {
    let container_name = format!("yunexal-{}", uuid);
    match state.docker.restart_container(&container_name, None::<RestartContainerOptions>).await {
        Ok(_) => Ok(Json("restarted".to_string())),
        Err(e) => {
            eprintln!("Failed to restart container: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_container(
    State(state): State<NodeState>,
    Path(uuid): Path<String>,
) -> Result<Json<String>, StatusCode> {
    let container_name = format!("yunexal-{}", uuid);

    // Stop container
    let stop_opts = Some(StopContainerOptions { t: Some(10), ..Default::default() });
    // Ignore error if already stopped or not found
    let _ = state.docker.stop_container(&container_name, stop_opts).await;

    // Remove container
    let remove_opts = Some(RemoveContainerOptions {
        force: true,
        ..Default::default()
    });

    match state.docker.remove_container(&container_name, remove_opts).await {
        Ok(_) => Ok(Json("deleted".to_string())),
        Err(e) => {
            eprintln!("Failed to delete container: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn console_handler(
    State(state): State<NodeState>,
    Path(uuid): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, uuid))
}

async fn handle_socket(mut socket: WebSocket, state: NodeState, uuid: String) {
    let container_name = format!("yunexal-{}", uuid);

    let options = Some(AttachContainerOptions {
        stdin: true,
        stdout: true,
        stderr: true,
        stream: true,
        logs: true,
        ..Default::default()
    });

    match state.docker.attach_container(&container_name, options).await {
        Ok(io) => {
            let (mut ws_sender, mut ws_receiver) = socket.split();
            let mut container_output = io.output;
            let mut container_input = io.input;

            // Task to forward container output to WebSocket
            let mut send_task = tokio::spawn(async move {
                while let Some(Ok(msg)) = container_output.next().await {
                    let text = msg.to_string(); 
                    if ws_sender.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
            });

            // Task to forward WebSocket input to container
            let mut recv_task = tokio::spawn(async move {
                while let Some(Ok(msg)) = ws_receiver.next().await {
                    if let Message::Text(text) = msg {
                        if container_input.write_all(text.as_bytes()).await.is_err() {
                            break;
                        }
                    } else if let Message::Close(_) = msg {
                         break;
                    }
                }
            });
            
            tokio::select! {
                _ = (&mut send_task) => recv_task.abort(),
                _ = (&mut recv_task) => send_task.abort(),
            };
        }
        Err(e) => {
            let _ = socket
                .send(Message::Text(format!("Error attaching to container: {}", e).into()))
                .await;
        }
    }
}

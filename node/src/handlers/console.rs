use crate::state::NodeState;
use axum::{
    extract::{Path, State, ws::{Message, WebSocket, WebSocketUpgrade}},
    response::IntoResponse,
};
use bollard::container::AttachContainerOptions;
use futures_util::{StreamExt, SinkExt};
use tokio::io::AsyncWriteExt;
use tracing::{error, info};

pub async fn console_handler(
    ws: WebSocketUpgrade,
    Path(uuid): Path<String>,
    State(state): State<NodeState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_console(socket, state, uuid))
}

async fn handle_console(socket: WebSocket, state: NodeState, uuid: String) {
    let container_name = format!("yunexal-{}", uuid);
    info!("Console: Attaching to container: {}", container_name);

    let options = Some(AttachContainerOptions::<String> {
        stdin: Some(true),
        stdout: Some(true),
        stderr: Some(true),
        stream: Some(true),
        logs: Some(true),
        ..Default::default()
    });

    match state.docker.attach_container(&container_name, options).await {
        Ok(io) => {
            let (mut ws_tx, mut ws_rx) = socket.split();
            let mut container_output = io.output;
            let mut container_input = io.input;

            // Task: Container output -> WebSocket
            let mut out_task = tokio::spawn(async move {
                while let Some(Ok(msg)) = container_output.next().await {
                    let text = msg.to_string();
                    if ws_tx.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
            });

            // Task: WebSocket -> Container input
            let mut in_task = tokio::spawn(async move {
                while let Some(Ok(msg)) = ws_rx.next().await {
                    match msg {
                        Message::Text(t) => {
                            let _ = container_input.write_all(t.as_bytes()).await;
                        }
                        Message::Binary(b) => {
                            let _ = container_input.write_all(&b).await;
                        }
                        _ => {}
                    }
                }
            });

            // Wait for one to finish
            tokio::select! {
                _ = &mut out_task => in_task.abort(),
                _ = &mut in_task => out_task.abort(),
            }
        }
        Err(e) => {
            error!("Console: Failed to attach to container {}: {}", container_name, e);
        }
    }
}

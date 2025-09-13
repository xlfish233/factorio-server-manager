use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::{extract::State, routing::get, Router};
use futures_util::{SinkExt, StreamExt};
use tracing::{debug, info, trace, warn};

use crate::state::AppState;

#[derive(serde::Deserialize)]
struct WsControls {
    r#type: String,
    value: String,
}

#[derive(serde::Deserialize)]
struct WsEnvelope {
    room_name: Option<String>,
    message: Option<serde_json::Value>,
    controls: Option<WsControls>,
}

#[derive(serde::Serialize)]
struct OutEnvelope<'a, T: serde::Serialize> {
    room_name: &'a str,
    #[serde(bound(deserialize = "T: serde::Deserialize<'de>"))]
    message: T,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/ws", get(ws_handler))
}

async fn ws_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl axum::response::IntoResponse {
    info!("WS handshake requested at /ws");
    ws.on_upgrade(move |socket| handle_socket(state, socket))
}

async fn handle_socket(state: AppState, socket: WebSocket) {
    info!("WebSocket connection established");
    // Writer channel for forwarding messages to socket (already JSON-encoded)
    let (tx, mut rx_out) = tokio::sync::mpsc::unbounded_channel::<String>();
    // Split socket into sender/receiver
    let (mut sender, mut receiver) = socket.split();
    // Writer task
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx_out.recv().await {
            trace!(len = msg.len(), "WS -> client");
            if let Err(e) = sender.send(Message::Text(msg.into())).await {
                debug!(error=?e, "WS send failed; likely closed");
                break;
            }
        }
    });

    let mut sub_status: Option<tokio::sync::broadcast::Receiver<String>> =
        Some(state.server_status_tx.subscribe());
    let mut sub_gamelog: Option<tokio::sync::broadcast::Receiver<String>> = None;

    // Default subscribe to server_status to avoid client race (subscribe-after-broadcast)
    let snap = serde_json::to_string(&*state.server.read().await).unwrap_or_else(|_| "{}".into());
    let env = OutEnvelope {
        room_name: "server_status",
        message: snap,
    };
    if let Ok(s) = serde_json::to_string(&env) {
        let _ = tx.send(s);
    }

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(txt))) => {
                        trace!(len = txt.len(), "WS <- client text");
                        if let Ok(env) = serde_json::from_str::<WsEnvelope>(&txt) {
                            trace!(raw=%txt, "WS <- controls envelope parsed");
                            // Touch optional fields to avoid unused field warnings
                            let _ = (&env.room_name, &env.message);
                            if let Some(ctrl) = env.controls {
                                match ctrl.r#type.as_str() {
                                "subscribe" => {
                                    match ctrl.value.as_str() {
                                        "server_status" => {
                                            sub_status = Some(state.server_status_tx.subscribe());
                                            info!("WS subscribed: server_status");
                                            // initial snapshot (send twice with small delay to avoid client handler race)
                                            let snap = serde_json::to_string(&*state.server.read().await).unwrap_or_else(|_| "{}".into());
                                            let env1 = OutEnvelope { room_name: "server_status", message: snap.clone() };
                                            if let Ok(s1) = serde_json::to_string(&env1) { let _ = tx.send(s1); }
                                            let tx_clone = tx.clone();
                                            tokio::spawn(async move {
                                                use tokio::time::{sleep, Duration};
                                                sleep(Duration::from_millis(100)).await;
                                                let env2 = OutEnvelope { room_name: "server_status", message: snap };
                                                if let Ok(s2) = serde_json::to_string(&env2) { let _ = tx_clone.send(s2); }
                                            });
                                        }
                                        "gamelog" => {
                                            sub_gamelog = Some(state.gamelog_tx.subscribe());
                                            debug!("Subscribed to gamelog");
                                            let cache = state.console_cache.lock().await;
                                            info!(count=cache.len(), "WS subscribed: gamelog (replay cache)");
                                            for line in cache.iter() {
                                                let env = OutEnvelope { room_name: "gamelog", message: line };
                                                if let Ok(s) = serde_json::to_string(&env) { let _ = tx.send(s); }
                                            }
                                        }
                                        other => { debug!(topic=%other, "Subscribe requested for unknown topic"); }
                                    }
                                }
                                "unsubscribe" => {
                                    match ctrl.value.as_str() {
                                        "server_status" => { sub_status = None; debug!("Unsubscribed from server_status"); }
                                        "gamelog" => { sub_gamelog = None; debug!("Unsubscribed from gamelog"); }
                                        other => { debug!(topic=%other, "Unsubscribe requested for unknown topic"); }
                                    }
                                }
                                "command" => {
                                    // Only attempt if server is running
                                    if state.server.read().await.running {
                                        let cmd = ctrl.value;
                                        if !cmd.is_empty() {
                                            info!(%cmd, "RCON command requested");
                                            let _ = crate::factorio::rcon_send(&state, &cmd).await;
                                            // No immediate echo; results should appear in gamelog
                                        }
                                    }
                                }
                                other => { debug!(r#type=%other, "Unknown control type"); }
                            }
                            }
                        } else {
                            warn!("Failed to parse incoming WS JSON envelope");
                        }
                    }
                    Some(Ok(Message::Close(frame))) => {
                        debug!(?frame, "WS close requested by client");
                        break
                    }
                    None => break,
                    Some(Ok(other)) => { trace!(?other, "WS <- client non-text message"); }
                    Some(Err(e)) => { warn!(error=?e, "WS receive error"); }
                }
            }
            // server_status forward
            Ok(payload) = async {
                match &mut sub_status { Some(rx) => rx.recv().await, None => futures_util::future::pending().await }
            } => {
                trace!(src="server_status", len=payload.len(), "Forwarding broadcast");
                // Wrap to { room_name, message }
                let env = OutEnvelope { room_name: "server_status", message: &payload };
                if let Ok(s) = serde_json::to_string(&env) { let _ = tx.send(s); }
            }
            // gamelog forward
            Ok(payload) = async {
                match &mut sub_gamelog { Some(rx) => rx.recv().await, None => futures_util::future::pending().await }
            } => {
                trace!(src="gamelog", len=payload.len(), "Forwarding broadcast");
                let env = OutEnvelope { room_name: "gamelog", message: &payload };
                if let Ok(s) = serde_json::to_string(&env) { let _ = tx.send(s); }
            }
        }
    }
    writer.abort();
    info!("WebSocket connection closed");
}

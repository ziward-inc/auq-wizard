use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use tauri::{menu::MenuItem, AppHandle, Emitter, Manager};
use tokio::{net::UnixListener, sync::Notify};
use tokio_util::codec::{Framed, LinesCodec};

use crate::{
    database::Database,
    protocol::{
        encode_frame, AnswerPayload, ClientMessage, QueueSummary, RequestStatus, ServerMessage,
        StoredRequest, MAX_FRAME_BYTES, PROTOCOL_VERSION,
    },
};

pub const BUNDLE_IDENTIFIER: &str = "com.ziward.auq-wizard";

pub struct Broker {
    pub database: Database,
    notify: Notify,
    shutting_down: AtomicBool,
    pending_item: Mutex<Option<MenuItem<tauri::Wry>>>,
}

impl Broker {
    pub fn new(database: Database) -> Self {
        Self {
            database,
            notify: Notify::new(),
            shutting_down: AtomicBool::new(false),
            pending_item: Mutex::new(None),
        }
    }

    pub fn set_pending_item(&self, item: MenuItem<tauri::Wry>) {
        if let Ok(mut pending_item) = self.pending_item.lock() {
            *pending_item = Some(item);
        }
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    pub fn request_shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn changed(&self, app: &AppHandle) {
        self.notify.notify_waiters();
        if let Ok(summary) = self.database.summary() {
            let _ = app.emit("queue-changed", &summary);
            if let Some(tray) = app.tray_by_id("main-tray") {
                let routing = if crate::preferences::is_enabled().unwrap_or(false) {
                    format!("{} pending", summary.pending)
                } else {
                    format!("paused · {} pending", summary.pending)
                };
                let _ = tray.set_tooltip(Some(format!("AUQ Wizard · {routing}")));
            }
            if let Ok(pending_item) = self.pending_item.lock() {
                if let Some(item) = pending_item.as_ref() {
                    let _ = item.set_text(format!("Pending: {}", summary.pending));
                }
            }
        }
    }
}

pub struct AppState {
    pub broker: Arc<Broker>,
}

pub fn socket_path() -> PathBuf {
    #[cfg(unix)]
    let user_id = unsafe { libc::geteuid() };
    #[cfg(not(unix))]
    let user_id = 0;
    PathBuf::from(format!("/tmp/auq-wizard-{user_id}/auq.sock"))
}

pub async fn run_socket_server(app: AppHandle, broker: Arc<Broker>) -> Result<()> {
    let path = socket_path();
    let directory = path.parent().context("socket path has no parent")?;
    fs::create_dir_all(directory)?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("failed to remove stale socket {}", path.display()))?;
    }

    let listener =
        UnixListener::bind(&path).with_context(|| format!("failed to bind {}", path.display()))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    log::info!("AUQ broker listening at {}", path.display());

    loop {
        let (stream, _) = listener.accept().await?;
        let app = app.clone();
        let broker = Arc::clone(&broker);
        tauri::async_runtime::spawn(async move {
            if let Err(error) = handle_client(stream, app, broker).await {
                log::warn!("AUQ client disconnected: {error:#}");
            }
        });
    }
}

async fn handle_client(
    stream: tokio::net::UnixStream,
    app: AppHandle,
    broker: Arc<Broker>,
) -> Result<()> {
    let codec = LinesCodec::new_with_max_length(MAX_FRAME_BYTES);
    let mut framed = Framed::new(stream, codec);
    let line = framed
        .next()
        .await
        .context("client closed before sending a request")??;
    let message: ClientMessage = serde_json::from_str(&line).context("invalid client message")?;
    if let Err(error) = message.validate_version() {
        send_error(&mut framed, "unsupported_version", error.to_string()).await?;
        return Ok(());
    }
    if matches!(&message, ClientMessage::Ask { .. }) {
        match crate::preferences::is_enabled() {
            Ok(true) => {}
            Ok(false) => {
                send_error(
                    &mut framed,
                    "routing_disabled",
                    "AUQ GUI routing is disabled; use native agent interaction",
                )
                .await?;
                return Ok(());
            }
            Err(error) => {
                send_error(&mut framed, "preferences_error", error.to_string()).await?;
                return Ok(());
            }
        }
    }

    match message {
        ClientMessage::Ask {
            request_id,
            payload,
            ..
        } => match broker.database.insert_or_get(&request_id, &payload) {
            Ok(request) => {
                send(
                    &mut framed,
                    &ServerMessage::Ack {
                        version: PROTOCOL_VERSION,
                        request_id: request_id.clone(),
                        status: request.status,
                    },
                )
                .await?;
                broker.changed(&app);
                if request.status == RequestStatus::Pending {
                    show_main_window(&app);
                }
                wait_for_result(&mut framed, &request_id, broker).await?;
            }
            Err(error) => send_error(&mut framed, "invalid_request", error.to_string()).await?,
        },
        ClientMessage::Wait { request_id, .. } => {
            if broker.database.get(&request_id)?.is_none() {
                send_error(
                    &mut framed,
                    "not_found",
                    format!("request {request_id} was not found"),
                )
                .await?;
            } else {
                wait_for_result(&mut framed, &request_id, broker).await?;
            }
        }
        ClientMessage::Status { request_id, .. } => {
            send(
                &mut framed,
                &ServerMessage::Status {
                    version: PROTOCOL_VERSION,
                    request: broker.database.get(&request_id)?,
                },
            )
            .await?;
        }
        ClientMessage::Cancel { request_id, .. } => match broker.database.cancel(&request_id) {
            Ok(request) => {
                broker.changed(&app);
                send(
                    &mut framed,
                    &ServerMessage::Result {
                        version: PROTOCOL_VERSION,
                        request_id,
                        status: request.status,
                        result: request.result,
                    },
                )
                .await?;
            }
            Err(error) => send_error(&mut framed, "cancel_failed", error.to_string()).await?,
        },
    }
    Ok(())
}

async fn wait_for_result(
    framed: &mut Framed<tokio::net::UnixStream, LinesCodec>,
    request_id: &str,
    broker: Arc<Broker>,
) -> Result<()> {
    loop {
        let request = broker
            .database
            .get(request_id)?
            .with_context(|| format!("request {request_id} disappeared"))?;
        if request.status != RequestStatus::Pending {
            send(
                framed,
                &ServerMessage::Result {
                    version: PROTOCOL_VERSION,
                    request_id: request_id.to_string(),
                    status: request.status,
                    result: request.result,
                },
            )
            .await?;
            return Ok(());
        }
        if broker.is_shutting_down() {
            send(
                framed,
                &ServerMessage::HostShutdown {
                    version: PROTOCOL_VERSION,
                    request_id: request_id.to_string(),
                },
            )
            .await?;
            return Ok(());
        }
        tokio::select! {
            _ = broker.notify.notified() => {},
            _ = tokio::time::sleep(Duration::from_secs(1)) => {},
        }
    }
}

async fn send(
    framed: &mut Framed<tokio::net::UnixStream, LinesCodec>,
    message: &ServerMessage,
) -> Result<()> {
    framed.send(encode_frame(message)?).await?;
    Ok(())
}

async fn send_error(
    framed: &mut Framed<tokio::net::UnixStream, LinesCodec>,
    code: impl Into<String>,
    message: impl Into<String>,
) -> Result<()> {
    send(
        framed,
        &ServerMessage::Error {
            version: PROTOCOL_VERSION,
            code: code.into(),
            message: message.into(),
        },
    )
    .await
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[tauri::command]
pub fn get_active_request(
    state: tauri::State<'_, AppState>,
) -> Result<Option<StoredRequest>, String> {
    state
        .broker
        .database
        .active()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_queue_summary(state: tauri::State<'_, AppState>) -> Result<QueueSummary, String> {
    state
        .broker
        .database
        .summary()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn submit_answer(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    request_id: String,
    result: AnswerPayload,
) -> Result<StoredRequest, String> {
    let request = state
        .broker
        .database
        .answer(&request_id, &result)
        .map_err(|error| error.to_string())?;
    state.broker.changed(&app);
    Ok(request)
}

#[tauri::command]
pub fn cancel_request(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    request_id: String,
) -> Result<StoredRequest, String> {
    let request = state
        .broker
        .database
        .cancel(&request_id)
        .map_err(|error| error.to_string())?;
    state.broker.changed(&app);
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_path_is_short_and_user_scoped() {
        let path = socket_path();
        assert!(path.to_string_lossy().contains("auq-wizard-"));
        assert!(path.as_os_str().len() < 100);
    }
}

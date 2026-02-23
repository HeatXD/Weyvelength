use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::oneshot;
use tonic::transport::Channel;

use crate::commands::messaging::GLOBAL_SESSION_ID;
use crate::commands::sessions::SessionInfoPayload;
use crate::grpc::{
    weyvelength::{StreamGlobalMembersRequest, StreamMessagesRequest, StreamSessionUpdatesRequest},
    WeyvelengthClient,
};
use crate::state::{AppState, StreamKind};

#[derive(Serialize, Clone)]
pub struct ChatMessagePayload {
    pub username: String,
    pub content: String,
    pub timestamp: i64,
}

/// Shared task for global chat stream.
/// `on_disconnect`: if Some, that event name is emitted when the stream ends
/// without being cancelled (i.e. unexpected server drop).
fn spawn_stream_task(
    app: AppHandle,
    channel: Channel,
    session_id: String,
    username: String,
    event_name: &'static str,
    cancel_rx: oneshot::Receiver<()>,
    on_disconnect: Option<&'static str>,
) {
    tauri::async_runtime::spawn(async move {
        let mut client = WeyvelengthClient::new(channel);
        let resp = match client
            .stream_messages(StreamMessagesRequest { session_id, username })
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Stream error ({event_name}): {e}");
                if let Some(ev) = on_disconnect {
                    let _ = app.emit(ev, ());
                }
                return;
            }
        };
        let mut stream = resp.into_inner();
        tokio::pin!(cancel_rx);
        let mut cancelled = false;
        loop {
            tokio::select! {
                _ = &mut cancel_rx => { cancelled = true; break; }
                msg = stream.message() => match msg {
                    Ok(Some(m)) => {
                        let _ = app.emit(event_name, ChatMessagePayload {
                            username: m.username,
                            content: m.content,
                            timestamp: m.timestamp,
                        });
                    }
                    _ => break,
                }
            }
        }
        if !cancelled {
            if let Some(ev) = on_disconnect {
                let _ = app.emit(ev, ());
            }
        }
    });
}

/// Dedicated task for the session-list update stream.
fn spawn_session_updates_task(
    app: AppHandle,
    channel: Channel,
    cancel_rx: oneshot::Receiver<()>,
) {
    tauri::async_runtime::spawn(async move {
        let mut client = WeyvelengthClient::new(channel);
        let resp = match client
            .stream_session_updates(StreamSessionUpdatesRequest {})
            .await
        {
            Ok(r) => r,
            Err(e) => { eprintln!("Session updates stream error: {e}"); return; }
        };
        let mut stream = resp.into_inner();
        tokio::pin!(cancel_rx);
        loop {
            tokio::select! {
                _ = &mut cancel_rx => break,
                msg = stream.message() => match msg {
                    Ok(Some(m)) => {
                        let sessions: Vec<SessionInfoPayload> = m
                            .sessions
                            .into_iter()
                            .map(SessionInfoPayload::from)
                            .collect();
                        let _ = app.emit("session-update", sessions);
                    }
                    _ => break,
                }
            }
        }
    });
}

/// Dedicated task for the global presence (online member) stream.
fn spawn_global_members_task(
    app: AppHandle,
    channel: Channel,
    cancel_rx: oneshot::Receiver<()>,
) {
    tauri::async_runtime::spawn(async move {
        let mut client = WeyvelengthClient::new(channel);
        let resp = match client
            .stream_global_members(StreamGlobalMembersRequest {})
            .await
        {
            Ok(r) => r,
            Err(e) => { eprintln!("Global members stream error: {e}"); return; }
        };
        let mut stream = resp.into_inner();
        tokio::pin!(cancel_rx);
        loop {
            tokio::select! {
                _ = &mut cancel_rx => break,
                msg = stream.message() => match msg {
                    Ok(Some(m)) => { let _ = app.emit("global-members", m.members); }
                    _ => break,
                }
            }
        }
    });
}

// ── commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn start_global_stream(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let rx = state.reset_stream(StreamKind::Global);
    spawn_stream_task(
        app,
        state.get_channel()?,
        GLOBAL_SESSION_ID.into(),
        state.get_username()?,
        "global-message",
        rx,
        Some("connection-lost"),
    );
    Ok(())
}

#[tauri::command]
pub async fn stop_global_stream(state: State<'_, AppState>) -> Result<(), String> {
    state.cancel_stream(StreamKind::Global);
    Ok(())
}

#[tauri::command]
pub async fn start_global_members_stream(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let rx = state.reset_stream(StreamKind::GlobalMembers);
    spawn_global_members_task(app, state.get_channel()?, rx);
    Ok(())
}

#[tauri::command]
pub async fn stop_global_members_stream(state: State<'_, AppState>) -> Result<(), String> {
    state.cancel_stream(StreamKind::GlobalMembers);
    Ok(())
}

#[tauri::command]
pub async fn start_session_updates_stream(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let rx = state.reset_stream(StreamKind::SessionUpdates);
    spawn_session_updates_task(app, state.get_channel()?, rx);
    Ok(())
}

#[tauri::command]
pub async fn stop_session_updates_stream(state: State<'_, AppState>) -> Result<(), String> {
    state.cancel_stream(StreamKind::SessionUpdates);
    Ok(())
}

#[tauri::command]
pub async fn start_session_stream(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    let rx = state.reset_stream(StreamKind::SessionMessages);
    spawn_stream_task(
        app,
        state.get_channel()?,
        session_id,
        state.get_username()?,
        "session-message",
        rx,
        None,
    );
    Ok(())
}

#[tauri::command]
pub async fn stop_session_stream(state: State<'_, AppState>) -> Result<(), String> {
    state.cancel_stream(StreamKind::SessionMessages);
    Ok(())
}

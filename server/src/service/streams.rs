use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tonic::{Response, Status};

use crate::codegen::weyvelength::{
    ChatMessage, GlobalMembersEvent, SessionsUpdatedEvent, StreamGlobalMembersRequest,
    StreamMessagesRequest, StreamSessionUpdatesRequest,
};
use crate::state::{GLOBAL_SESSION_ID, SharedState};

use super::helpers::public_sessions;

pub type MessagesStream = UnboundedReceiverStream<Result<ChatMessage, Status>>;
pub type SessionUpdatesStream = UnboundedReceiverStream<Result<SessionsUpdatedEvent, Status>>;
pub type GlobalMembersStream = UnboundedReceiverStream<Result<GlobalMembersEvent, Status>>;

pub async fn handle_stream_messages(
    state: &SharedState,
    req: StreamMessagesRequest,
) -> Result<Response<MessagesStream>, Status> {
    let session_id = req.session_id.clone();
    let username = req.username.clone();
    let is_global = session_id == GLOBAL_SESSION_ID;

    // Manage global member tracking first (before subscribing to the broadcast).
    let newly_added = if is_global {
        let mut entry = state
            .global_stream_refs
            .entry(username.clone())
            .or_insert(0);
        *entry += 1;
        let first = *entry == 1;
        drop(entry); // release shard lock before the next .await
        if first {
            if let Some(global) = state.sessions.get(GLOBAL_SESSION_ID) {
                global.inner.lock().await.members.insert(username.clone());
            }
        }
        first
    } else {
        false
    };

    // Subscribe to the session's broadcast channel (shard lock held only for the subscribe call).
    let broadcast_rx = state
        .sessions
        .get(&session_id)
        .ok_or_else(|| Status::not_found("Session not found"))?
        .tx
        .subscribe();

    if newly_added {
        println!("[global] {} connected", username);
        let _ = state.global_members_tx.send(());
    }

    let (tx, rx) = mpsc::unbounded_channel::<Result<ChatMessage, Status>>();
    let state_clone = state.clone();

    tokio::spawn(async move {
        let mut broadcast_rx = broadcast_rx;

        let client_disconnected = loop {
            tokio::select! {
                _ = tx.closed() => break true,
                result = broadcast_rx.recv() => match result {
                    Ok(msg) => {
                        if tx.send(Ok(msg)).is_err() {
                            break true;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break false,
                }
            }
        };

        if client_disconnected && is_global {
            // Decrement ref count; remove from global.members when it reaches zero.
            let mut entry = state_clone
                .global_stream_refs
                .entry(username.clone())
                .or_insert(0);
            if *entry > 0 {
                *entry -= 1;
            }
            let should_remove = *entry == 0;
            drop(entry); // release shard lock before the next .await
            if should_remove {
                state_clone.global_stream_refs.remove(&username);
                if let Some(global) = state_clone.sessions.get(GLOBAL_SESSION_ID) {
                    global.inner.lock().await.members.remove(&username);
                }
                let _ = state_clone.global_members_tx.send(());
                println!("[global] {} disconnected", username);
            }
        }
        // Non-global: no implicit leave. Lifecycle managed by JoinSession/LeaveSession.
    });

    Ok(Response::new(UnboundedReceiverStream::new(rx)))
}

pub async fn handle_stream_session_updates(
    state: &SharedState,
    _req: StreamSessionUpdatesRequest,
) -> Result<Response<SessionUpdatesStream>, Status> {
    let update_rx = state.session_update_tx.subscribe();
    let (tx, rx) = mpsc::unbounded_channel::<Result<SessionsUpdatedEvent, Status>>();
    let state_clone = state.clone();

    tokio::spawn(async move {
        let initial = public_sessions(&state_clone).await;
        if tx
            .send(Ok(SessionsUpdatedEvent { sessions: initial }))
            .is_err()
        {
            return;
        }

        let mut update_rx = update_rx;
        loop {
            tokio::select! {
                _ = tx.closed() => break,
                result = update_rx.recv() => match result {
                    Ok(()) => {
                        let sessions = public_sessions(&state_clone).await;
                        if tx.send(Ok(SessionsUpdatedEvent { sessions })).is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        }
    });

    Ok(Response::new(UnboundedReceiverStream::new(rx)))
}

pub async fn handle_stream_global_members(
    state: &SharedState,
    _req: StreamGlobalMembersRequest,
) -> Result<Response<GlobalMembersStream>, Status> {
    // Subscribe first so we don't miss an update between reading initial state
    // and starting the loop.
    let members_rx = state.global_members_tx.subscribe();

    let initial = match state.sessions.get(GLOBAL_SESSION_ID) {
        Some(global) => global
            .inner
            .lock()
            .await
            .members
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        None => vec![],
    };

    let (tx, rx) = mpsc::unbounded_channel::<Result<GlobalMembersEvent, Status>>();

    if tx
        .send(Ok(GlobalMembersEvent { members: initial }))
        .is_err()
    {
        return Ok(Response::new(UnboundedReceiverStream::new(rx)));
    }

    let state_clone = state.clone();
    tokio::spawn(async move {
        let mut members_rx = members_rx;
        loop {
            tokio::select! {
                _ = tx.closed() => break,
                result = members_rx.recv() => match result {
                    Ok(()) => {
                        let members = match state_clone.sessions.get(GLOBAL_SESSION_ID) {
                            Some(global) => global
                                .inner
                                .lock()
                                .await
                                .members
                                .iter()
                                .cloned()
                                .collect::<Vec<_>>(),
                            None => vec![],
                        };
                        if tx.send(Ok(GlobalMembersEvent { members })).is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        }
    });

    Ok(Response::new(UnboundedReceiverStream::new(rx)))
}

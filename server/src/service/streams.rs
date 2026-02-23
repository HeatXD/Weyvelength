use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tonic::{Response, Status};

use crate::codegen::weyvelength::{
    ChatMessage, GlobalMembersEvent, SessionInfo, SessionsUpdatedEvent, StreamGlobalMembersRequest,
    StreamMessagesRequest, StreamSessionUpdatesRequest,
};
use crate::session::leave_session_inner;
use crate::state::{GLOBAL_SESSION_ID, SharedState};

use super::helpers::{notify_sessions_changed, public_sessions};

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

    let (broadcast_rx, global_members_tx, newly_added) = {
        let mut state = state.lock().unwrap();
        let newly_added = if is_global {
            let ref_count = state
                .global_stream_refs
                .entry(username.clone())
                .or_insert(0);
            *ref_count += 1;
            if *ref_count == 1 {
                let global = state.sessions.get_mut(GLOBAL_SESSION_ID).unwrap();
                global.members.push(username.clone());
                true
            } else {
                false
            }
        } else {
            false
        };
        let rx = state
            .sessions
            .get(&session_id)
            .ok_or_else(|| Status::not_found("Session not found"))?
            .tx
            .subscribe();
        (rx, state.global_members_tx.clone(), newly_added)
    };

    if newly_added {
        println!("[global] {} connected", username);
        let _ = global_members_tx.send(());
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

        if client_disconnected {
            if is_global {
                let mut state = state_clone.lock().unwrap();
                let remove = {
                    let ref_count = state
                        .global_stream_refs
                        .entry(username.clone())
                        .or_insert(0);
                    if *ref_count > 0 {
                        *ref_count -= 1;
                    }
                    *ref_count == 0
                };
                if remove {
                    state.global_stream_refs.remove(&username);
                    let global = state.sessions.get_mut(GLOBAL_SESSION_ID).unwrap();
                    global.members.retain(|m| m != &username);
                    let _ = state.global_members_tx.send(());
                    println!("[global] {} disconnected", username);
                }
            } else {
                let update_tx = {
                    let mut state = state_clone.lock().unwrap();
                    let was_public = state
                        .sessions
                        .get(&session_id)
                        .map(|s| s.is_public)
                        .unwrap_or(false);
                    leave_session_inner(&mut *state, &session_id, &username);
                    was_public.then(|| state.session_update_tx.clone())
                };
                if let Some(tx) = update_tx {
                    notify_sessions_changed(&tx);
                }
            }
        }
    });

    Ok(Response::new(UnboundedReceiverStream::new(rx)))
}

pub async fn handle_stream_session_updates(
    state: &SharedState,
    _req: StreamSessionUpdatesRequest,
) -> Result<Response<SessionUpdatesStream>, Status> {
    let update_rx = {
        let state = state.lock().unwrap();
        state.session_update_tx.subscribe()
    };

    let (tx, rx) = mpsc::unbounded_channel::<Result<SessionsUpdatedEvent, Status>>();
    let state_clone = state.clone();

    tokio::spawn(async move {
        let initial = {
            let state = state_clone.lock().unwrap();
            public_sessions(&state)
        };
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
                        let sessions: Vec<SessionInfo> = {
                            let state = state_clone.lock().unwrap();
                            public_sessions(&state)
                        };
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
    let (members_rx, initial) = {
        let state = state.lock().unwrap();
        let rx = state.global_members_tx.subscribe();
        let members = state
            .sessions
            .get(GLOBAL_SESSION_ID)
            .map(|s| s.members.clone())
            .unwrap_or_default();
        (rx, members)
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
                        let members = {
                            let state = state_clone.lock().unwrap();
                            state
                                .sessions
                                .get(GLOBAL_SESSION_ID)
                                .map(|s| s.members.clone())
                                .unwrap_or_default()
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

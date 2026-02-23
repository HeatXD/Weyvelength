use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tonic::{Response, Status};

use crate::codegen::weyvelength::{
    SendSignalRequest, SendSignalResponse, Signal, SignalKind, StreamSignalsRequest,
};
use crate::session::{LeaveInfo, leave_session_inner};
use crate::state::SharedState;

use super::helpers::notify_sessions_changed;

pub async fn handle_send_signal(
    state: &SharedState,
    req: SendSignalRequest,
) -> Result<Response<SendSignalResponse>, Status> {
    let signal = req
        .signal
        .ok_or_else(|| Status::invalid_argument("Missing signal"))?;

    let Some(session) = state
        .sessions
        .get(&signal.session_id)
        .map(|r| r.value().clone())
    else {
        return Ok(Response::new(SendSignalResponse {}));
    };
    let sender = session
        .inner
        .lock()
        .await
        .signal_senders
        .get(&signal.to_user)
        .cloned();

    if let Some(tx) = sender {
        let kind_label = match signal.kind {
            k if k == SignalKind::SdpOffer as i32 => "offer",
            k if k == SignalKind::SdpAnswer as i32 => "answer",
            k if k == SignalKind::IceCandidate as i32 => "ice",
            _ => "unknown",
        };
        println!(
            "[signal] {} {} → {} in {}",
            kind_label, signal.from_user, signal.to_user, signal.session_id
        );
        let _ = tx.send(Arc::new(signal));
    }

    Ok(Response::new(SendSignalResponse {}))
}

pub type SignalsStream = UnboundedReceiverStream<Result<Signal, Status>>;

pub async fn handle_stream_signals(
    state: &SharedState,
    req: StreamSignalsRequest,
) -> Result<Response<SignalsStream>, Status> {
    let username = req.username.clone();
    let session_id = req.session_id.clone();

    let (bridge_tx, bridge_rx) = mpsc::unbounded_channel::<Arc<Signal>>();

    // Register this user's signal sender in the session's inner map.
    let session = state
        .sessions
        .get(&session_id)
        .ok_or_else(|| Status::not_found("Session not found"))?
        .value()
        .clone();
    session
        .inner
        .lock()
        .await
        .signal_senders
        .insert(username.clone(), bridge_tx);

    println!("[signal stream] opened for {} in {}", username, session_id);

    // Bridge channel: Arc<Signal> (fan-out end) → Result<Signal, Status> (tonic end).
    let (tonic_tx, tonic_rx) = mpsc::unbounded_channel::<Result<Signal, Status>>();
    let state_clone = state.clone();

    tokio::spawn(async move {
        let mut bridge_rx = bridge_rx;
        loop {
            tokio::select! {
                _ = tonic_tx.closed() => break,
                msg = bridge_rx.recv() => match msg {
                    Some(sig) => {
                        // Unwrap Arc: if this is the sole ref, avoid a clone.
                        if tonic_tx.send(Ok(Arc::unwrap_or_clone(sig))).is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
        }

        // Remove signal sender so no one tries to route signals to a closed channel.
        if let Some(session) = state_clone.sessions.get(&session_id) {
            let mut inner = session.inner.lock().await;
            inner.signal_senders.remove(&username);
        }

        // Treat stream close as implicit leave (handles crashes and clients that
        // disconnect without calling LeaveSession). leave_session_inner is an
        // atomic gate — if an explicit LeaveSession already ran, this returns None.
        if let Some(LeaveInfo {
            senders,
            is_public,
            new_host,
        }) = leave_session_inner(&state_clone, &session_id, &username).await
        {
            if is_public {
                notify_sessions_changed(&state_clone.session_update_tx);
            }

            if !senders.is_empty() {
                let sig = Arc::new(Signal {
                    from_user: username.clone(),
                    to_user: String::new(),
                    session_id: session_id.clone(),
                    kind: SignalKind::MemberLeft as i32,
                    payload: username.clone(),
                });
                for tx in senders {
                    let _ = tx.send(Arc::clone(&sig));
                }
            }

            if let Some((new_host_name, host_senders)) = new_host {
                let host_sig = Arc::new(Signal {
                    from_user: String::new(),
                    to_user: String::new(),
                    session_id: session_id.clone(),
                    kind: SignalKind::HostChanged as i32,
                    payload: new_host_name.clone(),
                });
                for tx in host_senders {
                    let _ = tx.send(Arc::clone(&host_sig));
                }
                println!(
                    "[session] host of {} migrated to {} (implicit)",
                    session_id, new_host_name
                );
            }
        }

        println!("[signal stream] closed for {} in {}", username, session_id);
    });

    Ok(Response::new(UnboundedReceiverStream::new(tonic_rx)))
}

use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tonic::{Response, Status};

use crate::codegen::weyvelength::{
    SendSignalRequest, SendSignalResponse, Signal, SignalKind, StreamSignalsRequest,
};
use crate::session::leave_session_inner;
use crate::state::SharedState;

use super::helpers::notify_sessions_changed;

pub async fn handle_send_signal(
    state: &SharedState,
    req: SendSignalRequest,
) -> Result<Response<SendSignalResponse>, Status> {
    let signal = req
        .signal
        .ok_or_else(|| Status::invalid_argument("Missing signal"))?;

    let sender = {
        let state = state.lock().unwrap();
        state
            .sessions
            .get(&signal.session_id)
            .and_then(|s| s.signal_senders.get(&signal.to_user).cloned())
    };

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
        let _ = tx.send(signal); // ignore send error; peer stream may have closed
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

    let (bridge_tx, bridge_rx) = mpsc::unbounded_channel::<Signal>();

    {
        let mut state = state.lock().unwrap();
        let session = state
            .sessions
            .get_mut(&session_id)
            .ok_or_else(|| Status::not_found("Session not found"))?;
        session.signal_senders.insert(username.clone(), bridge_tx);
    }

    println!("[signal stream] opened for {} in {}", username, session_id);

    let (tonic_tx, tonic_rx) = mpsc::unbounded_channel::<Result<Signal, Status>>();
    let state_clone = state.clone();

    tokio::spawn(async move {
        let mut bridge_rx = bridge_rx;
        loop {
            tokio::select! {
                _ = tonic_tx.closed() => break,
                msg = bridge_rx.recv() => match msg {
                    Some(signal) => {
                        if tonic_tx.send(Ok(signal)).is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
        // Remove signal sender entry and check whether the member was still
        // listed (i.e. they didn't already leave via handle_leave_session).
        // If they were still a member, collect the remaining senders so we can
        // fan-out MemberLeft after the implicit leave below.
        let leave_senders: Vec<_> = {
            let mut state = state_clone.lock().unwrap();
            if let Some(session) = state.sessions.get_mut(&session_id) {
                session.signal_senders.remove(&username);
                if session.members.contains(&username) {
                    // Still a member — collect peers to notify
                    session.signal_senders.values().cloned().collect()
                } else {
                    // Already left explicitly; no notification needed
                    vec![]
                }
            } else {
                vec![]
            }
        };

        // Treat signal stream close as implicit leave — handles crashes and
        // clients that disconnect without calling LeaveSession explicitly.
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

        // Fan-out MemberLeft to remaining members (only fires for crash/disconnect;
        // explicit leave already did this in handle_leave_session).
        if !leave_senders.is_empty() {
            let sig = Signal {
                from_user: username.clone(),
                to_user: String::new(),
                session_id: session_id.clone(),
                kind: SignalKind::MemberLeft as i32,
                payload: username.clone(),
            };
            for tx in leave_senders {
                let _ = tx.send(sig.clone());
            }
        }

        println!("[signal stream] closed for {} in {}", username, session_id);
    });

    Ok(Response::new(UnboundedReceiverStream::new(tonic_rx)))
}

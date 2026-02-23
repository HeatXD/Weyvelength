use std::sync::Arc;

use tokio::sync::{broadcast, mpsc};

use crate::codegen::weyvelength::{SessionInfo, Signal, SignalKind};
use crate::state::{GLOBAL_SESSION_ID, ServerState, SessionData};

/// Build the current list of public (non-global) sessions.
/// Collects Arc refs first (brief shard-lock hold), then awaits each inner lock
/// separately so no DashMap shard lock is held across an await point.
pub async fn public_sessions(state: &ServerState) -> Vec<SessionInfo> {
    let candidates: Vec<Arc<SessionData>> = state
        .sessions
        .iter()
        .filter(|e| e.id != GLOBAL_SESSION_ID && e.is_public)
        .map(|e| Arc::clone(e.value()))
        .collect();

    let mut result = Vec::new();
    for session in candidates {
        let inner = session.inner.lock().await;
        result.push(SessionInfo {
            id: session.id.clone(),
            name: session.name.clone(),
            member_count: inner.members.len() as u32,
            is_public: session.is_public,
            max_members: session.max_members,
        });
    }
    result
}

/// Notify all `StreamSessionUpdates` subscribers that the session list changed.
/// Call this **after** releasing any locks.
pub fn notify_sessions_changed(tx: &broadcast::Sender<()>) {
    // Subscriber may have dropped; not an error.
    let _ = tx.send(());
}

/// Build a single `Signal` and fan it out to all senders in `senders`.
/// Ignores send errors — a closed receiver just means the peer disconnected.
pub fn fanout_signal(
    senders: &[mpsc::UnboundedSender<Arc<Signal>>],
    kind: SignalKind,
    from: &str,
    session_id: &str,
    payload: &str,
) {
    let sig = Arc::new(Signal {
        from_user: from.to_owned(),
        to_user: String::new(),
        session_id: session_id.to_owned(),
        kind: kind as i32,
        payload: payload.to_owned(),
    });
    for tx in senders {
        // Subscriber may have dropped; not an error.
        let _ = tx.send(Arc::clone(&sig));
    }
}

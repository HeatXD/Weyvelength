use std::sync::Arc;

use tokio::sync::broadcast;

use crate::codegen::weyvelength::SessionInfo;
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
    let _ = tx.send(());
}

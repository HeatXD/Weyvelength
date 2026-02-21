use tokio::sync::broadcast;

use crate::codegen::weyvelength::SessionInfo;
use crate::state::{ServerState, GLOBAL_SESSION_ID};

/// Build the current list of public (non-global) sessions.
pub fn public_sessions(state: &ServerState) -> Vec<SessionInfo> {
    state
        .sessions
        .values()
        .filter(|s| s.id != GLOBAL_SESSION_ID && s.is_public)
        .map(|s| SessionInfo {
            id: s.id.clone(),
            name: s.name.clone(),
            member_count: s.members.len() as u32,
            is_public: s.is_public,
            max_members: s.max_members,
        })
        .collect()
}

/// Notify all `StreamSessionUpdates` subscribers that the session list changed.
/// Call this **after** releasing the mutex lock.
pub fn notify_sessions_changed(tx: &broadcast::Sender<()>) {
    let _ = tx.send(());
}

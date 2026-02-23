use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::mpsc;
use uuid::Uuid;

use crate::codegen::weyvelength::Signal;
use crate::state::{GLOBAL_SESSION_ID, ServerState};

const LOBBY_CODE_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

pub fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

pub fn generate_lobby_code(state: &ServerState) -> String {
    loop {
        let bytes = Uuid::new_v4();
        let candidate: String = bytes.as_bytes()[..8]
            .iter()
            .map(|b| LOBBY_CODE_CHARS[(b % 36) as usize] as char)
            .collect();
        if !state.sessions.contains_key(&candidate) {
            return candidate;
        }
    }
}

/// Return value of `leave_session_inner` when the leave succeeds (first caller wins).
pub struct LeaveInfo {
    /// Signal senders for all remaining peers (excluding the leaver).
    pub senders: Vec<mpsc::UnboundedSender<Arc<Signal>>>,
    /// Whether the session was public (caller may want to notify session list).
    pub is_public: bool,
    /// Set when the leaver was the host and remaining members exist.
    /// Tuple: (new_host_username, all_remaining_signal_senders).
    pub new_host: Option<(String, Vec<mpsc::UnboundedSender<Arc<Signal>>>)>,
}

/// Remove user from session, migrate host if needed, delete session if empty.
/// Returns `Some(LeaveInfo)` if this is the first (authoritative) leave call,
/// `None` if the user was already gone (duplicate / race loser).
pub async fn leave_session_inner(
    state: &ServerState,
    session_id: &str,
    username: &str,
) -> Option<LeaveInfo> {
    // Atomic gate: only remove if the user is actually mapped to this session.
    // First caller wins; concurrent duplicate (e.g. explicit leave racing with
    // stream-close implicit leave) gets None and skips fan-out.
    if state
        .user_session_index
        .remove_if(username, |_, v| v.as_str() == session_id)
        .is_none()
    {
        return None;
    }

    let info = if let Some(session) = state.sessions.get(session_id) {
        let mut inner = session.inner.lock().await;

        let was_host = inner.host == username;
        inner.members.remove(username);

        // Collect senders for remaining peers (for MemberLeft fan-out).
        let senders: Vec<_> = inner
            .signal_senders
            .iter()
            .filter(|(k, _)| k.as_str() != username)
            .map(|(_, tx)| tx.clone())
            .collect();

        // Migrate host if needed.
        let new_host = if was_host && !inner.members.is_empty() {
            let nh = inner.members.iter().next().cloned().unwrap();
            inner.host = nh.clone();
            let nh_senders: Vec<_> = inner.signal_senders.values().cloned().collect();
            Some((nh, nh_senders))
        } else {
            None
        };

        let is_empty = inner.members.is_empty();
        let is_global = session_id == GLOBAL_SESSION_ID;
        let is_public = session.is_public;
        drop(inner);

        if is_empty && !is_global {
            drop(session);
            state.sessions.remove(session_id);
        }

        LeaveInfo {
            senders,
            is_public,
            new_host,
        }
    } else {
        LeaveInfo {
            senders: vec![],
            is_public: false,
            new_host: None,
        }
    };

    Some(info)
}

/// Auto-leave previous session then join the new one.
/// Returns `Err` if the target session does not exist.
pub async fn join_session_inner(
    state: &ServerState,
    session_id: &str,
    username: &str,
) -> Result<(), String> {
    // Auto-leave any previous session.
    if let Some((_, old_id)) = state.user_session_index.remove(username) {
        if let Some(old_session) = state.sessions.get(&old_id) {
            let mut inner = old_session.inner.lock().await;
            inner.members.remove(username);
            let is_empty = inner.members.is_empty();
            let is_global = old_id == GLOBAL_SESSION_ID;
            drop(inner);
            if is_empty && !is_global {
                drop(old_session);
                state.sessions.remove(&old_id);
            }
        }
    }

    let session = state
        .sessions
        .get(session_id)
        .ok_or_else(|| format!("Session '{}' not found", session_id))?;

    session
        .inner
        .lock()
        .await
        .members
        .insert(username.to_string());

    state
        .user_session_index
        .insert(username.to_string(), session_id.to_string());
    Ok(())
}

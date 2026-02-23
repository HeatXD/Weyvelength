use std::time::{SystemTime, UNIX_EPOCH};

use uuid::Uuid;

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

/// Remove user from session; delete session if empty (never deletes __global__).
pub fn leave_session_inner(state: &mut ServerState, session_id: &str, username: &str) {
    if let Some(session) = state.sessions.get_mut(session_id) {
        session.members.retain(|m| m != username);
        let is_empty = session.members.is_empty();
        let is_global = session_id == GLOBAL_SESSION_ID;
        if is_empty && !is_global {
            state.sessions.remove(session_id);
        }
    }
    state.user_session_index.remove(username);
}

/// Auto-leave previous session, then join new one. Returns Err if session not found.
pub fn join_session_inner(
    state: &mut ServerState,
    session_id: &str,
    username: &str,
) -> Result<(), String> {
    if let Some(old_id) = state.user_session_index.get(username).cloned() {
        leave_session_inner(state, &old_id, username);
    }

    let session = state
        .sessions
        .get_mut(session_id)
        .ok_or_else(|| format!("Session '{}' not found", session_id))?;

    if !session.members.contains(&username.to_string()) {
        session.members.push(username.to_string());
    }
    state
        .user_session_index
        .insert(username.to_string(), session_id.to_string());
    Ok(())
}

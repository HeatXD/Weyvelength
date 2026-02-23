use tokio::sync::broadcast;
use tonic::{Response, Status};

use crate::codegen::weyvelength::{
    CreateSessionRequest, CreateSessionResponse, GetMembersRequest, GetMembersResponse,
    JoinSessionRequest, JoinSessionResponse, LeaveSessionRequest, LeaveSessionResponse,
    ListSessionsRequest, ListSessionsResponse, Signal, SignalKind,
};
use crate::session::{generate_lobby_code, join_session_inner, leave_session_inner};
use crate::state::{BROADCAST_CAPACITY, SessionData, SharedState};

use super::helpers::{notify_sessions_changed, public_sessions};

pub async fn handle_list_sessions(
    state: &SharedState,
    _req: ListSessionsRequest,
) -> Result<Response<ListSessionsResponse>, Status> {
    let state = state.lock().unwrap();
    Ok(Response::new(ListSessionsResponse {
        sessions: public_sessions(&state),
    }))
}

pub async fn handle_create_session(
    state: &SharedState,
    req: CreateSessionRequest,
) -> Result<Response<CreateSessionResponse>, Status> {
    let (code, is_public, max_members, update_tx) = {
        let mut state = state.lock().unwrap();
        let code = generate_lobby_code(&state);
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        let max_members = req.max_members.max(2);

        state.sessions.insert(
            code.clone(),
            SessionData {
                id: code.clone(),
                name: code.clone(),
                host: req.username.clone(),
                members: vec![],
                is_public: req.is_public,
                max_members,
                tx,
                signal_senders: std::collections::HashMap::new(),
            },
        );

        join_session_inner(&mut *state, &code, &req.username).map_err(Status::internal)?;

        let update_tx = state.session_update_tx.clone();
        (code, req.is_public, max_members, update_tx)
    };

    if is_public {
        notify_sessions_changed(&update_tx);
    }

    let host = req.username.clone();
    let visibility = if is_public { "public" } else { "private" };
    println!(
        "[session] created {} by {} ({}, max={})",
        code, req.username, visibility, max_members
    );

    Ok(Response::new(CreateSessionResponse {
        session_id: code.clone(),
        session_name: code,
        is_public,
        max_members,
        existing_peers: vec![],
        host,
    }))
}

pub async fn handle_join_session(
    state: &SharedState,
    req: JoinSessionRequest,
) -> Result<Response<JoinSessionResponse>, Status> {
    let (session_id, session_name, is_public, max_members, existing_peers, host, update_tx) = {
        let mut state = state.lock().unwrap();

        {
            let session = state
                .sessions
                .get(&req.session_id)
                .ok_or_else(|| Status::not_found("Session not found"))?;
            if session.max_members > 0 && session.members.len() as u32 >= session.max_members {
                return Err(Status::resource_exhausted("Session is full"));
            }
        }

        join_session_inner(&mut *state, &req.session_id, &req.username)
            .map_err(Status::not_found)?;

        let session = state.sessions.get(&req.session_id).unwrap();
        let existing_peers: Vec<String> = session
            .members
            .iter()
            .filter(|m| *m != &req.username)
            .cloned()
            .collect();
        (
            session.id.clone(),
            session.name.clone(),
            session.is_public,
            session.max_members,
            existing_peers,
            session.host.clone(),
            state.session_update_tx.clone(),
        )
    };

    if is_public {
        notify_sessions_changed(&update_tx);
    }

    // Fan-out MemberJoined to all members who already have a signal stream open.
    // The joiner's own stream isn't registered yet (they open it after this RPC
    // returns), so all current senders are for the pre-existing members.
    {
        let state = state.lock().unwrap();
        if let Some(session) = state.sessions.get(&session_id) {
            let sig = Signal {
                from_user: req.username.clone(),
                to_user: String::new(),
                session_id: session_id.clone(),
                kind: SignalKind::MemberJoined as i32,
                payload: req.username.clone(),
            };
            for tx in session.signal_senders.values() {
                let _ = tx.send(sig.clone());
            }
        }
    }

    println!(
        "[session] {} joined {} ({} existing peer{})",
        req.username,
        session_id,
        existing_peers.len(),
        if existing_peers.len() == 1 { "" } else { "s" }
    );

    Ok(Response::new(JoinSessionResponse {
        session_id,
        session_name,
        is_public,
        max_members,
        existing_peers,
        host,
    }))
}

pub async fn handle_leave_session(
    state: &SharedState,
    req: LeaveSessionRequest,
) -> Result<Response<LeaveSessionResponse>, Status> {
    // Collect signal senders for all members except the leaver BEFORE removing
    // them, so we can notify them after the member is gone.
    let (update_tx, leave_senders, host_changed) = {
        let mut state = state.lock().unwrap();
        // If the user isn't a member the RPC is a duplicate (e.g. two failing
        // WebRTC connections both triggered leave on the client side). Return
        // early so we don't fan-out a second MemberLeft signal.
        if !state
            .sessions
            .get(&req.session_id)
            .map(|s| s.members.contains(&req.username))
            .unwrap_or(false)
        {
            return Ok(Response::new(LeaveSessionResponse {}));
        }
        let was_host = state
            .sessions
            .get(&req.session_id)
            .map(|s| s.host == req.username)
            .unwrap_or(false);
        let leave_senders: Vec<_> = state
            .sessions
            .get(&req.session_id)
            .map(|s| {
                s.signal_senders
                    .iter()
                    .filter(|(user, _)| *user != &req.username)
                    .map(|(_, tx)| tx.clone())
                    .collect()
            })
            .unwrap_or_default();
        let was_public = state
            .sessions
            .get(&req.session_id)
            .map(|s| s.is_public)
            .unwrap_or(false);
        leave_session_inner(&mut *state, &req.session_id, &req.username);
        // After leave, migrate host if needed and session still has members.
        let host_changed: Option<(String, Vec<_>)> = if was_host {
            state.sessions.get_mut(&req.session_id).and_then(|session| {
                session.members.first().cloned().map(|new_host| {
                    session.host = new_host.clone();
                    let senders: Vec<_> = session.signal_senders.values().cloned().collect();
                    (new_host, senders)
                })
            })
        } else {
            None
        };
        (
            was_public.then(|| state.session_update_tx.clone()),
            leave_senders,
            host_changed,
        )
    };

    if let Some(tx) = update_tx {
        notify_sessions_changed(&tx);
    }

    // Fan-out MemberLeft to remaining members.
    let sig = Signal {
        from_user: req.username.clone(),
        to_user: String::new(),
        session_id: req.session_id.clone(),
        kind: SignalKind::MemberLeft as i32,
        payload: req.username.clone(),
    };
    for tx in &leave_senders {
        let _ = tx.send(sig.clone());
    }

    // Fan-out HostChanged if the host migrated.
    if let Some((new_host, host_senders)) = host_changed {
        let host_sig = Signal {
            from_user: String::new(),
            to_user: String::new(),
            session_id: req.session_id.clone(),
            kind: SignalKind::HostChanged as i32,
            payload: new_host.clone(),
        };
        for tx in host_senders {
            let _ = tx.send(host_sig.clone());
        }
        println!(
            "[session] host of {} migrated to {}",
            req.session_id, new_host
        );
    }

    println!("[session] {} left {}", req.username, req.session_id);
    Ok(Response::new(LeaveSessionResponse {}))
}

pub async fn handle_get_members(
    state: &SharedState,
    req: GetMembersRequest,
) -> Result<Response<GetMembersResponse>, Status> {
    let state = state.lock().unwrap();
    let session = state
        .sessions
        .get(&req.session_id)
        .ok_or_else(|| Status::not_found("Session not found"))?;
    Ok(Response::new(GetMembersResponse {
        members: session.members.clone(),
    }))
}

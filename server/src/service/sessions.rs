use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::{Mutex, broadcast};
use tonic::{Response, Status};

use crate::codegen::weyvelength::{
    CreateSessionRequest, CreateSessionResponse, GetMembersRequest, GetMembersResponse,
    JoinSessionRequest, JoinSessionResponse, LeaveSessionRequest, LeaveSessionResponse,
    ListSessionsRequest, ListSessionsResponse, Signal, SignalKind,
};
use crate::session::{LeaveInfo, generate_lobby_code, join_session_inner, leave_session_inner};
use crate::state::{
    BROADCAST_CAPACITY, SESSION_MAX_MEMBERS, SESSION_MIN_MEMBERS, SessionData, SessionInner,
    SharedState,
};

use super::helpers::{notify_sessions_changed, public_sessions};

pub async fn handle_list_sessions(
    state: &SharedState,
    _req: ListSessionsRequest,
) -> Result<Response<ListSessionsResponse>, Status> {
    Ok(Response::new(ListSessionsResponse {
        sessions: public_sessions(state).await,
    }))
}

pub async fn handle_create_session(
    state: &SharedState,
    req: CreateSessionRequest,
) -> Result<Response<CreateSessionResponse>, Status> {
    let code = generate_lobby_code(state);
    let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
    let max_members = req
        .max_members
        .clamp(SESSION_MIN_MEMBERS, SESSION_MAX_MEMBERS);

    let session = Arc::new(SessionData {
        id: code.clone(),
        name: code.clone(),
        is_public: req.is_public,
        max_members,
        tx,
        inner: Mutex::new(SessionInner {
            host: req.username.clone(),
            members: HashSet::new(),
            signal_senders: HashMap::new(),
        }),
    });

    state.sessions.insert(code.clone(), session);

    join_session_inner(state, &code, &req.username)
        .await
        .map_err(Status::internal)?;

    if req.is_public {
        notify_sessions_changed(&state.session_update_tx);
    }

    println!(
        "[session] created {} by {} ({}, max={})",
        code,
        req.username,
        if req.is_public { "public" } else { "private" },
        max_members
    );

    Ok(Response::new(CreateSessionResponse {
        session_id: code.clone(),
        session_name: code,
        is_public: req.is_public,
        max_members,
        existing_peers: vec![],
        host: req.username,
    }))
}

pub async fn handle_join_session(
    state: &SharedState,
    req: JoinSessionRequest,
) -> Result<Response<JoinSessionResponse>, Status> {
    // Clone the Arc out of DashMap immediately so the shard lock isn't held
    // across any await points below.
    let session = state
        .sessions
        .get(&req.session_id)
        .ok_or_else(|| Status::not_found("Session not found"))?
        .value()
        .clone();

    {
        let inner = session.inner.lock().await;
        if session.max_members > 0 && inner.members.len() as u32 >= session.max_members {
            return Err(Status::resource_exhausted("Session is full"));
        }
    }

    join_session_inner(state, &req.session_id, &req.username)
        .await
        .map_err(Status::not_found)?;

    // Collect mutable state needed for the response and fan-out.
    let (existing_peers, host, senders) = {
        let inner = session.inner.lock().await;
        let existing_peers: Vec<String> = inner
            .members
            .iter()
            .filter(|m| *m != &req.username)
            .cloned()
            .collect();
        let senders: Vec<_> = inner.signal_senders.values().cloned().collect();
        (existing_peers, inner.host.clone(), senders)
    };

    if session.is_public {
        notify_sessions_changed(&state.session_update_tx);
    }

    // Fan-out MemberJoined to peers who already have a signal stream open.
    // The joiner's own stream isn't registered yet (opened after this RPC returns).
    let sig = Arc::new(Signal {
        from_user: req.username.clone(),
        to_user: String::new(),
        session_id: session.id.clone(),
        kind: SignalKind::MemberJoined as i32,
        payload: req.username.clone(),
    });
    for tx in &senders {
        let _ = tx.send(Arc::clone(&sig));
    }

    println!(
        "[session] {} joined {} ({} existing peer{})",
        req.username,
        session.id,
        existing_peers.len(),
        if existing_peers.len() == 1 { "" } else { "s" }
    );

    Ok(Response::new(JoinSessionResponse {
        session_id: session.id.clone(),
        session_name: session.name.clone(),
        is_public: session.is_public,
        max_members: session.max_members,
        existing_peers,
        host,
    }))
}

pub async fn handle_leave_session(
    state: &SharedState,
    req: LeaveSessionRequest,
) -> Result<Response<LeaveSessionResponse>, Status> {
    // Atomic remove: first caller wins, duplicate (e.g. two WebRTC failures both
    // triggering leave) gets None and returns immediately.
    let Some(LeaveInfo {
        senders,
        is_public,
        new_host,
    }) = leave_session_inner(state, &req.session_id, &req.username).await
    else {
        return Ok(Response::new(LeaveSessionResponse {}));
    };

    if is_public {
        notify_sessions_changed(&state.session_update_tx);
    }

    // Fan-out MemberLeft to remaining peers.
    let sig = Arc::new(Signal {
        from_user: req.username.clone(),
        to_user: String::new(),
        session_id: req.session_id.clone(),
        kind: SignalKind::MemberLeft as i32,
        payload: req.username.clone(),
    });
    for tx in &senders {
        let _ = tx.send(Arc::clone(&sig));
    }

    // Fan-out HostChanged if the host migrated.
    if let Some((new_host_name, host_senders)) = new_host {
        let host_sig = Arc::new(Signal {
            from_user: String::new(),
            to_user: String::new(),
            session_id: req.session_id.clone(),
            kind: SignalKind::HostChanged as i32,
            payload: new_host_name.clone(),
        });
        for tx in &host_senders {
            let _ = tx.send(Arc::clone(&host_sig));
        }
        println!(
            "[session] host of {} migrated to {}",
            req.session_id, new_host_name
        );
    }

    println!("[session] {} left {}", req.username, req.session_id);
    Ok(Response::new(LeaveSessionResponse {}))
}

pub async fn handle_get_members(
    state: &SharedState,
    req: GetMembersRequest,
) -> Result<Response<GetMembersResponse>, Status> {
    let session = state
        .sessions
        .get(&req.session_id)
        .ok_or_else(|| Status::not_found("Session not found"))?
        .value()
        .clone();
    let members = session.inner.lock().await.members.iter().cloned().collect();
    Ok(Response::new(GetMembersResponse { members }))
}

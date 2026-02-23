use serde::Serialize;
use tauri::State;

use crate::grpc::{
    weyvelength::{
        CreateSessionRequest, GetMembersRequest, JoinSessionRequest, LeaveSessionRequest,
        ListSessionsRequest,
    },
    WeyvelengthClient,
};
use crate::state::AppState;

#[derive(Serialize, Clone)]
pub struct SessionInfoPayload {
    pub id: String,
    pub name: String,
    pub member_count: u32,
    pub is_public: bool,
    pub max_members: u32,
}

#[derive(Serialize, Clone)]
pub struct SessionPayload {
    pub session_id: String,
    pub session_name: String,
    pub is_public: bool,
    pub max_members: u32,
    pub existing_peers: Vec<String>,
    pub host: String,
}

#[tauri::command]
pub async fn list_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<SessionInfoPayload>, String> {
    let mut client = WeyvelengthClient::new(state.get_channel()?);
    let response = client
        .list_sessions(ListSessionsRequest {})
        .await
        .map_err(|e| e.to_string())?;
    let sessions = response
        .into_inner()
        .sessions
        .into_iter()
        .map(|s| SessionInfoPayload {
            id: s.id,
            name: s.name,
            member_count: s.member_count,
            is_public: s.is_public,
            max_members: s.max_members,
        })
        .collect();
    Ok(sessions)
}

#[tauri::command]
pub async fn create_session(
    state: State<'_, AppState>,
    is_public: bool,
    max_members: u32,
) -> Result<SessionPayload, String> {
    let username = state.get_username()?;
    let mut client = WeyvelengthClient::new(state.get_channel()?);
    let response = client
        .create_session(CreateSessionRequest { username, is_public, max_members })
        .await
        .map_err(|e| e.to_string())?;
    let r = response.into_inner();
    Ok(SessionPayload {
        session_id: r.session_id,
        session_name: r.session_name,
        is_public: r.is_public,
        max_members: r.max_members,
        existing_peers: r.existing_peers,
        host: r.host,
    })
}

#[tauri::command]
pub async fn join_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<SessionPayload, String> {
    let username = state.get_username()?;
    let mut client = WeyvelengthClient::new(state.get_channel()?);
    let response = client
        .join_session(JoinSessionRequest { session_id, username })
        .await
        .map_err(|e| e.to_string())?;
    let r = response.into_inner();
    Ok(SessionPayload {
        session_id: r.session_id,
        session_name: r.session_name,
        is_public: r.is_public,
        max_members: r.max_members,
        existing_peers: r.existing_peers,
        host: r.host,
    })
}

#[tauri::command]
pub async fn leave_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    let username = state.get_username()?;
    let mut client = WeyvelengthClient::new(state.get_channel()?);
    client
        .leave_session(LeaveSessionRequest { session_id, username })
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_members(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<String>, String> {
    let mut client = WeyvelengthClient::new(state.get_channel()?);
    let response = client
        .get_members(GetMembersRequest { session_id })
        .await
        .map_err(|e| e.to_string())?;
    Ok(response.into_inner().members)
}

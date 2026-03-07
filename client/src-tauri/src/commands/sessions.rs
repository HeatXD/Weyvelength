use serde::Serialize;
use tauri::State;

use crate::grpc::{
    weyvelength::{
        CreateSessionRequest, GetMembersRequest, JoinSessionRequest, LeaveSessionRequest,
        ListSessionsRequest, SessionInfo, StartGameRequest, StopGameRequest,
    },
    WeyvelengthClient,
};
use crate::state::AppState;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfoPayload {
    pub id: String,
    pub name: String,
    pub member_count: u32,
    pub is_public: bool,
    pub max_members: u32,
    pub game_started: bool,
}

impl From<SessionInfo> for SessionInfoPayload {
    fn from(s: SessionInfo) -> Self {
        SessionInfoPayload {
            id: s.id,
            name: s.name,
            member_count: s.member_count,
            is_public: s.is_public,
            max_members: s.max_members,
            game_started: s.game_started,
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionPayload {
    pub session_id: String,
    pub session_name: String,
    pub is_public: bool,
    pub max_members: u32,
    pub existing_peers: Vec<String>,
    pub host: String,
    pub game_started: bool,
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
        .map(SessionInfoPayload::from)
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
        .create_session(state.authed_request(CreateSessionRequest { username, is_public, max_members }))
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
        game_started: r.game_started,
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
        .join_session(state.authed_request(JoinSessionRequest { session_id, username }))
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
        game_started: r.game_started,
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
        .leave_session(state.authed_request(LeaveSessionRequest { session_id, username }))
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
        .get_members(state.authed_request(GetMembersRequest { session_id }))
        .await
        .map_err(|e| e.to_string())?;
    Ok(response.into_inner().members)
}

#[tauri::command]
pub async fn start_game(
    state: State<'_, AppState>,
    session_id: String,
    payload: String,
) -> Result<(), String> {
    let username = state.get_username()?;
    let mut client = WeyvelengthClient::new(state.get_channel()?);
    client
        .start_game(state.authed_request(StartGameRequest { session_id, username, payload }))
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn stop_game(state: State<'_, AppState>) -> Result<(), String> {
    let session_id = state
        .current_session_id
        .lock()
        .unwrap()
        .clone()
        .ok_or("Not in a session")?;
    let username = state.get_username()?;
    let mut client = WeyvelengthClient::new(state.get_channel()?);
    client
        .stop_game(state.authed_request(StopGameRequest { session_id, username }))
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn list_games(folder: String) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(&folder).map_err(|e| e.to_string())?;
    let mut games: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    games.sort();
    Ok(games)
}

#[tauri::command]
pub async fn hash_file(path: String) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let data = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
    let hash = Sha256::digest(&data);
    Ok(format!("{:x}", hash))
}

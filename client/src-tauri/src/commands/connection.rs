use serde::Serialize;
use tauri::State;
use tonic::transport::Channel;

use crate::grpc::{weyvelength::GetServerInfoRequest, WeyvelengthClient};
use crate::state::{AppState, IceServerEntry};

#[derive(Serialize, Clone)]
pub struct IceServerPayload {
    pub url: String,
    pub username: String,
    pub credential: String,
    pub name: String,
}

#[derive(Serialize, Clone)]
pub struct ServerInfoPayload {
    pub server_name: String,
    pub motd: String,
    pub ice_servers: Vec<IceServerPayload>,
}

#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    host: String,
    port: u16,
    username: String,
) -> Result<(), String> {
    let url = format!("http://{}:{}", host, port);
    let channel = Channel::from_shared(url)
        .map_err(|e| e.to_string())?
        .connect()
        .await
        .map_err(|e| e.to_string())?;
    *state.channel.write().unwrap() = Some(channel);
    *state.username.write().unwrap() = Some(username);
    Ok(())
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>) -> Result<(), String> {
    state.cancel_all_streams();
    state.close_all_peer_connections().await;
    *state.current_session_id.lock().unwrap() = None;
    *state.channel.write().unwrap() = None;
    *state.username.write().unwrap() = None;
    Ok(())
}

#[tauri::command]
pub async fn get_server_info(state: State<'_, AppState>) -> Result<ServerInfoPayload, String> {
    let mut client = WeyvelengthClient::new(state.get_channel()?);
    let response = client
        .get_server_info(GetServerInfoRequest {})
        .await
        .map_err(|e| e.to_string())?;
    let r = response.into_inner();

    // Single pass: clone fields into IceServerEntry for state, move originals into IceServerPayload.
    let (ice_entries, ice_servers): (Vec<IceServerEntry>, Vec<IceServerPayload>) = r
        .ice_servers
        .into_iter()
        .map(|s| {
            let entry = IceServerEntry {
                url: s.url.clone(),
                username: s.username.clone(),
                credential: s.credential.clone(),
                name: s.name.clone(),
            };
            let payload = IceServerPayload {
                url: s.url,
                username: s.username,
                credential: s.credential,
                name: s.name,
            };
            (entry, payload)
        })
        .unzip();
    *state.ice_servers.write().unwrap() = ice_entries;

    Ok(ServerInfoPayload {
        server_name: r.server_name,
        motd: r.motd,
        ice_servers,
    })
}

/// Set the preferred TURN server by name. Pass `None` to disable TURN (direct connection only).
#[tauri::command]
pub async fn set_turn_server(
    state: State<'_, AppState>,
    name: Option<String>,
) -> Result<(), String> {
    *state.selected_turn.write().unwrap() = name;
    Ok(())
}

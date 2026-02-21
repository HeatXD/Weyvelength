use tauri::State;

use crate::grpc::{weyvelength::SendMessageRequest, WeyvelengthClient};
use crate::state::AppState;

pub const GLOBAL_SESSION_ID: &str = "__global__";

async fn send(
    state: &State<'_, AppState>,
    session_id: String,
    content: String,
) -> Result<(), String> {
    let username = state.get_username()?;
    let mut client = WeyvelengthClient::new(state.get_channel()?);
    client
        .send_message(SendMessageRequest {
            session_id,
            username,
            content,
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn send_global_message(
    state: State<'_, AppState>,
    content: String,
) -> Result<(), String> {
    send(&state, GLOBAL_SESSION_ID.into(), content).await
}

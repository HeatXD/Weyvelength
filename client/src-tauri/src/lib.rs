mod commands;
mod grpc;
mod state;
mod webrtc;

use commands::{connection::{connect, disconnect, get_server_info, set_turn_server}, messaging::*, sessions::*, streaming::*, webrtc::*};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(state::AppState::new())
        .invoke_handler(tauri::generate_handler![
            connect,
            disconnect,
            get_server_info,
            set_turn_server,
            list_sessions,
            create_session,
            join_session,
            leave_session,
            get_members,
            send_global_message,
            start_global_stream,
            stop_global_stream,
            start_session_updates_stream,
            stop_session_updates_stream,
            start_global_members_stream,
            stop_global_members_stream,
            join_session_webrtc,
            leave_session_webrtc,
            close_peer_connection,
            send_session_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

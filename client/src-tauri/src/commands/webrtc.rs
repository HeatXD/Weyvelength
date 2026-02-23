use tauri::{AppHandle, Emitter, Manager, State};
use tonic::transport::Channel;
use tokio::sync::oneshot;

use crate::grpc::{
    weyvelength::{Signal, SignalKind, StreamSignalsRequest},
    WeyvelengthClient,
};
use crate::state::{AppState, StreamKind};
use crate::webrtc::{create_peer_connection, MemberEventPayload};
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;

// ── commands ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn join_session_webrtc(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    existing_peers: Vec<String>,
    force_relay: bool,
) -> Result<(), String> {
    *state.force_relay.lock().unwrap_or_else(|e| e.into_inner()) = force_relay;
    *state.current_session_id.lock().unwrap_or_else(|e| e.into_inner()) = Some(session_id.clone());
    state.close_all_peer_connections().await;

    // Open the signal stream BEFORE sending any offers so incoming answers
    // can be received and routed to the right peer connection.
    let cancel_rx = state.reset_stream(StreamKind::Signals);
    spawn_signal_stream_task(
        app.clone(),
        state.get_channel()?,
        cancel_rx,
        session_id.clone(),
        state.get_username()?,
    );

    let ice_servers = state.effective_ice_servers();

    let username = state.get_username()?;

    for peer in existing_peers {
        match create_peer_connection(
            app.clone(),
            username.clone(),
            peer.clone(),
            session_id.clone(),
            ice_servers.clone(),
            true,  // we are the initiator (new joiner)
            None,
            force_relay,
        )
        .await
        {
            Ok(entry) => {
                state
                    .peer_connections
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(peer, entry);
            }
            Err(e) => eprintln!("[WebRTC] create_peer_connection: {e}"),
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn close_peer_connection(
    state: State<'_, AppState>,
    peer: String,
) -> Result<(), String> {
    let entry = state
        .peer_connections
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&peer);
    if let Some(entry) = entry {
        let _ = entry.pc.close().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn leave_session_webrtc(state: State<'_, AppState>) -> Result<(), String> {
    state.cancel_stream(StreamKind::Signals);
    state.close_all_peer_connections().await;
    *state.current_session_id.lock().unwrap_or_else(|e| e.into_inner()) = None;
    Ok(())
}

// ── signal stream task ────────────────────────────────────────────────────────

fn spawn_signal_stream_task(
    app: AppHandle,
    channel: Channel,
    cancel_rx: oneshot::Receiver<()>,
    session_id: String,
    username: String,
) {
    tauri::async_runtime::spawn(async move {
        let mut client = WeyvelengthClient::new(channel);
        let resp = match client
            .stream_signals(StreamSignalsRequest { session_id, username })
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Signal stream error: {e}");
                return;
            }
        };
        let mut stream = resp.into_inner();
        tokio::pin!(cancel_rx);
        loop {
            tokio::select! {
                _ = &mut cancel_rx => break,
                msg = stream.message() => match msg {
                    Ok(Some(signal)) => handle_incoming_signal(app.clone(), signal).await,
                    _ => break,
                }
            }
        }
    });
}

// ── incoming signal dispatch ──────────────────────────────────────────────────

async fn handle_incoming_signal(app: AppHandle, signal: Signal) {
    let from = signal.from_user.clone();
    let state = app.state::<AppState>();

    if signal.kind == SignalKind::SdpOffer as i32 {
        // A new peer sent us an offer — become the answerer.
        let ice_servers = state.effective_ice_servers();
        let local_username = match state.get_username() {
            Ok(u) => u,
            Err(_) => return,
        };
        let session_id = match state
            .current_session_id
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            Some(s) => s,
            None => return,
        };
        let force_relay = *state.force_relay.lock().unwrap_or_else(|e| e.into_inner());

        match create_peer_connection(
            app.clone(),
            local_username,
            from.clone(),
            session_id,
            ice_servers,
            false,       // answerer
            Some(signal),
            force_relay,
        )
        .await
        {
            Ok(entry) => {
                state
                    .peer_connections
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(from, entry);
            }
            Err(e) => eprintln!("[WebRTC] answerer create_peer_connection: {e}"),
        }
    } else if signal.kind == SignalKind::SdpAnswer as i32 {
        // Route answer to the existing peer connection.
        let pc = {
            let map = state.peer_connections.lock().unwrap_or_else(|e| e.into_inner());
            map.get(&from).map(|e| e.pc.clone())
        };
        if let Some(pc) = pc {
            match RTCSessionDescription::answer(signal.payload) {
                Ok(sdp) => {
                    eprintln!("[ICE] received answer from {from}");
                    if let Err(e) = pc.set_remote_description(sdp).await {
                        eprintln!("[ICE] set_remote_description: {e}");
                    }
                }
                Err(e) => eprintln!("[ICE] bad answer SDP: {e}"),
            }
        }
    } else if signal.kind == SignalKind::IceCandidate as i32 {
        // Add a trickled ICE candidate to the existing peer connection.
        let pc = {
            let map = state.peer_connections.lock().unwrap_or_else(|e| e.into_inner());
            map.get(&from).map(|e| e.pc.clone())
        };
        if let Some(pc) = pc {
            match serde_json::from_str::<RTCIceCandidateInit>(&signal.payload) {
                Ok(init) => {
                    if let Err(e) = pc.add_ice_candidate(init).await {
                        eprintln!("[ICE] add_ice_candidate: {e}");
                    }
                }
                Err(e) => eprintln!("[ICE] bad candidate JSON: {e}"),
            }
        }
    } else if signal.kind == SignalKind::MemberJoined as i32 {
        let _ = app.emit(
            "member-event",
            MemberEventPayload { username: signal.payload, joined: true },
        );
    } else if signal.kind == SignalKind::MemberLeft as i32 {
        let _ = app.emit(
            "member-event",
            MemberEventPayload { username: signal.payload, joined: false },
        );
    } else if signal.kind == SignalKind::HostChanged as i32 {
        let _ = app.emit("host-changed", signal.payload);
    }
}

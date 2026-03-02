use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tonic::{self, transport::Channel};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, oneshot};

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
    state.force_relay.store(force_relay, Ordering::Relaxed);
    *state.current_session_id.lock().unwrap() = Some(session_id.clone());
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
        state.auth_token.read().unwrap().clone(),
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
                state.peer_connections.insert(peer, entry);
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
    let entry = state.peer_connections.remove(&peer).map(|(_, e)| e);
    if let Some(entry) = entry {
        let _ = entry.pc.close().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn leave_session_webrtc(state: State<'_, AppState>) -> Result<(), String> {
    state.cancel_stream(StreamKind::Signals);
    state.close_all_peer_connections().await;
    kill_game_process(&state);
    *state.current_session_id.lock().unwrap() = None;
    Ok(())
}

/// Launch the game/emulator executable and start the UDP proxy bridge.
/// The bridge routes data between the local emulator and peers via game_dc.
#[tauri::command]
pub async fn launch_game(
    app: AppHandle,
    state: State<'_, AppState>,
    exe_path: String,
    player_id: u32,
    config: String,
) -> Result<(), String> {
    // Kill any previously running game process before launching a new one.
    // This prevents orphaned watchers when the button is pressed multiple times.
    kill_game_process(&state);

    // Bind a UDP socket for the emulator ↔ WL bridge.
    let udp_sock = Arc::new(
        UdpSocket::bind("127.0.0.1:0")
            .await
            .map_err(|e| e.to_string())?,
    );
    let wl_port = udp_sock.local_addr().unwrap().port();

    // mpsc channel: game_dc on_message → UDP to emulator.
    let (bridge_tx, mut bridge_rx) = mpsc::channel::<bytes::Bytes>(256);
    *state.udp_bridge_tx.lock().unwrap() = Some(bridge_tx);

    // Task A: drain mpsc → UDP to emulator.
    // Emulator address is learned from the first inbound UDP packet (write-once).
    let sock_send = udp_sock.clone();
    let emulator_addr: Arc<std::sync::OnceLock<std::net::SocketAddr>> =
        Arc::new(std::sync::OnceLock::new());
    let emu_addr_r = emulator_addr.clone();
    tokio::spawn(async move {
        while let Some(data) = bridge_rx.recv().await {
            if let Some(addr) = emu_addr_r.get().copied() {
                let _ = sock_send.send_to(&data, addr).await;
            }
        }
    });

    // Parse config JSON to build player_id maps.
    // user_to_id: stored in AppState for on_message callbacks (name → id).
    // id_to_user: captured into Task B closure (id → name for routing).
    let (user_to_id, id_to_user): (HashMap<String, u8>, HashMap<u8, String>) = {
        let val: serde_json::Value = serde_json::from_str(&config)
            .unwrap_or(serde_json::Value::Null);
        let mut u2i = HashMap::new();
        let mut i2u = HashMap::new();
        if let Some(members) = val.get("members").and_then(|m| m.as_object()) {
            for (name, v) in members {
                if let Some(pid) = v.get("player_id").and_then(|p| p.as_u64()) {
                    let pid = pid as u8;
                    u2i.insert(name.clone(), pid);
                    if pid > 0 {
                        i2u.insert(pid, name.clone());
                    }
                }
            }
        }
        (u2i, i2u)
    };
    state.game_player_ids.clear();
    for (k, v) in user_to_id {
        state.game_player_ids.insert(k, v);
    }

    // Task B: listen for UDP from emulator → route to peer's game_dc.
    // Wire frame: [u8 to_player_id][game data]
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    *state.udp_listener_cancel.lock().unwrap() = Some(cancel_tx);
    let sock_recv = udp_sock.clone();
    let app2 = app.clone();
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];
        tokio::pin!(cancel_rx);
        loop {
            tokio::select! {
                _ = &mut cancel_rx => break,
                res = sock_recv.recv_from(&mut buf) => {
                    let Ok((len, addr)) = res else { break };
                    emulator_addr.get_or_init(|| addr);
                    if len < 1 { continue; }
                    let to_player_id = buf[0];
                    let data = bytes::Bytes::copy_from_slice(&buf[1..len]);
                    let Some(dest) = id_to_user.get(&to_player_id).cloned() else { continue };
                    let dc_slot = app2
                        .state::<AppState>()
                        .peer_connections
                        .get(&dest)
                        .map(|e| e.game_dc.clone());
                    if let Some(slot) = dc_slot {
                        tokio::spawn(async move {
                            if let Some(dc) = slot.lock().await.as_ref() {
                                let _ = dc.send(&data).await;
                            }
                        });
                    }
                }
            }
        }
    });

    eprintln!("[launch_game] spawning exe={exe_path:?} player_id={player_id} udp_port={wl_port}");
    let exe_dir = std::path::Path::new(&exe_path).parent();
    let mut cmd = tokio::process::Command::new(&exe_path);
    if let Some(dir) = exe_dir {
        cmd.current_dir(dir);
    }
    cmd.arg("--wl-udp-port").arg(wl_port.to_string())
        .arg("--wl-player-id").arg(player_id.to_string())
        .arg("--wl-config").arg(&config);
    #[cfg(windows)]
    cmd.creation_flags(0x00000010); // CREATE_NEW_CONSOLE
    let child = cmd.spawn()
        .map_err(|e| format!("Failed to launch: {e}"))?;
    eprintln!("[launch_game] spawned pid={:?}", child.id());

    // Watcher owns the child. On natural exit it cleans up and notifies the frontend.
    // On external kill (kill_game_process sends the cancel), it just kills the child.
    let (watch_tx, watch_rx) = oneshot::channel::<()>();
    *state.game_watch_cancel.lock().unwrap() = Some(watch_tx);
    let app_watch = app.clone();
    tokio::spawn(async move {
        let mut child = child;
        tokio::select! {
            _ = watch_rx => {
                eprintln!("[watcher] cancelled externally, killing child");
                let _ = child.start_kill();
            }
            status = child.wait() => {
                eprintln!("[watcher] process exited naturally: {status:?}");
                let state = app_watch.state::<AppState>();
                if let Some(tx) = state.udp_listener_cancel.lock().unwrap().take() {
                    let _ = tx.send(());
                }
                *state.udp_bridge_tx.lock().unwrap() = None;
                state.game_player_ids.clear();
                let _ = app_watch.emit("game-stopped", ());
            }
        }
    });

    Ok(())
}

// ── game process cleanup ──────────────────────────────────────────────────────

/// Tear down the UDP bridge and signal the process-watcher to kill the child.
/// Call from leave/disconnect teardown or on GAME_STOPPED signal.
pub fn kill_game_process(state: &AppState) {
    // Signal the watcher, it holds the child and will call start_kill().
    if let Some(tx) = state.game_watch_cancel.lock().unwrap().take() {
        let _ = tx.send(());
    }
    if let Some(tx) = state.udp_listener_cancel.lock().unwrap().take() {
        let _ = tx.send(());
    }
    *state.udp_bridge_tx.lock().unwrap() = None;
    state.game_player_ids.clear();
}

// ── signal stream task ────────────────────────────────────────────────────────

fn spawn_signal_stream_task(
    app: AppHandle,
    channel: Channel,
    cancel_rx: oneshot::Receiver<()>,
    session_id: String,
    username: String,
    token: Option<String>,
) {
    tauri::async_runtime::spawn(async move {
        let mut client = WeyvelengthClient::new(channel);
        let mut req = tonic::Request::new(StreamSignalsRequest { session_id, username });
        if let Some(t) = &token {
            if let Ok(val) = format!("bearer {t}").parse() {
                req.metadata_mut().insert("authorization", val);
            }
        }
        let resp = match client
            .stream_signals(req)
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
        // A new peer sent us an offer, become the answerer.
        let ice_servers = state.effective_ice_servers();
        let local_username = match state.get_username() {
            Ok(u) => u,
            Err(_) => return,
        };
        let Some(session_id) = state.current_session_id.lock().unwrap().clone() else { return };
        let force_relay = state.force_relay.load(Ordering::Relaxed);

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
                state.peer_connections.insert(from, entry);
            }
            Err(e) => eprintln!("[WebRTC] answerer create_peer_connection: {e}"),
        }
    } else if signal.kind == SignalKind::SdpAnswer as i32 {
        // Route answer to the existing peer connection.
        if let Some(pc) = state.get_peer_connection(&from) {
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
        if let Some(pc) = state.get_peer_connection(&from) {
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
    } else if signal.kind == SignalKind::GameStarted as i32 {
        let _ = app.emit("game-started", &signal.payload);
    } else if signal.kind == SignalKind::GameStopped as i32 {
        kill_game_process(&state);
        let _ = app.emit("game-stopped", ());
    }
}

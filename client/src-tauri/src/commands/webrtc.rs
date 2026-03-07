use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tonic::{self, transport::Channel};
use tokio::sync::{Mutex as TokioMutex, oneshot};
use webrtc::data_channel::RTCDataChannel;

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
    debug_log: bool,
) -> Result<(), String> {
    // Kill any previously running game process before launching a new one.
    // This prevents orphaned watchers when the button is pressed multiple times.
    kill_game_process(&state);

    // Bind a UDP socket for the emulator ↔ WL bridge.
    let udp_sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.map_err(|e| e.to_string())?;
    let wl_port = udp_sock.local_addr().unwrap().port();
    state.bridge_port.store(wl_port, Ordering::Relaxed);

    // Parse config JSON: name → player_id.
    let user_to_id: HashMap<String, u8> = {
        let val: serde_json::Value = serde_json::from_str(&config)
            .unwrap_or(serde_json::Value::Null);
        let mut u2i = HashMap::new();
        if let Some(members) = val.get("members").and_then(|m| m.as_object()) {
            for (name, v) in members {
                if let Some(pid) = v.get("playerId").and_then(|p| p.as_u64()) {
                    u2i.insert(name.clone(), pid as u8);
                }
            }
        }
        u2i
    };

    // Collect game_dc slots before any await (never hold DashMap ref across await).
    let dc_slots: Vec<(String, Arc<TokioMutex<Option<Arc<RTCDataChannel>>>>)> = state
        .peer_connections
        .iter()
        .map(|e| (e.key().clone(), e.value().game_dc.clone()))
        .collect();

    // Socket used by on_message closures to forward inbound WebRTC game data to
    // the emulator. Non-blocking because on_message is called synchronously from
    // a webrtc-rs internal thread and must not stall. Local UDP never fills the
    // send buffer, so EAGAIN/WSAEWOULDBLOCK is not expected in practice.
    let send_sock = Arc::new(
        std::net::UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?
    );
    send_sock.set_nonblocking(true).map_err(|e| e.to_string())?;

    // Reset emulator port so stale on_message closures from a previous game don't fire.
    state.emulator_port.store(0, Ordering::Relaxed);

    // Pre-build player_id → Arc<RTCDataChannel> map.
    let mut id_to_dc: HashMap<u8, Arc<RTCDataChannel>> = HashMap::new();
    for (username, slot) in dc_slots {
        if let Some(&pid) = user_to_id.get(&username) {
            if pid > 0 {
                if let Some(dc) = slot.lock().await.as_ref() {
                    id_to_dc.insert(pid, dc.clone());
                }
            }
        }
    }

    // ── Diagnostic log file (debug_log only) ─────────────────────────────────
    // Unique per-launch: timestamp_ms + PID so concurrent runs never collide.
    use std::io::Write as _;
    let t_start = std::time::Instant::now();
    let log: std::sync::Arc<std::sync::Mutex<Option<std::io::BufWriter<std::fs::File>>>> =
        std::sync::Arc::new(std::sync::Mutex::new(if debug_log {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let pid = std::process::id();
            let log_path = std::env::temp_dir().join(format!("wl_bridge_{ts}_{pid}.log"));
            eprintln!("[bridge] diagnostic log → {}", log_path.display());
            std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&log_path)
                .ok()
                .map(std::io::BufWriter::new)
        } else {
            None
        }));
    macro_rules! wlog {
        ($lg:expr, $($arg:tt)*) => {{
            if let Ok(mut g) = $lg.lock() {
                if let Some(ref mut f) = *g {
                    let ms = t_start.elapsed().as_millis();
                    let _ = writeln!(f, "[{ms:>8}ms] {}", format_args!($($arg)*));
                    let _ = f.flush();
                }
            }
        }};
    }

    // Inbound: WebRTC DC → emulator.
    // Wire format (batched): [u16 len LE][data][u16 len LE][data]...
    // on_message splits the batch and sends each sub-packet to the SDK socket.
    for (&pid, dc) in &id_to_dc {
        let sock = send_sock.clone();
        let app3 = app.clone();
        let log_in = log.clone();
        dc.on_message(Box::new(move |msg| {
            let port = app3.state::<AppState>().emulator_port.load(Ordering::Relaxed);
            if port != 0 {
                let data = &msg.data;
                let mut offset = 0usize;
                let mut count = 0u32;
                while offset + 2 <= data.len() {
                    let sub_len = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
                    offset += 2;
                    if offset + sub_len > data.len() { break; }
                    let sub_len_capped = sub_len.min(1400);
                    let mut frame = [0u8; 1 + 1400];
                    frame[0] = pid;
                    frame[1..1 + sub_len_capped].copy_from_slice(&data[offset..offset + sub_len_capped]);
                    let _ = sock.send_to(&frame[..1 + sub_len_capped], SocketAddr::from(([127, 0, 0, 1], port)));
                    offset += sub_len;
                    count += 1;
                }
                if count > 1 {
                    wlog!(log_in, "[in ] pid={pid} batch={count} pkts {}B", data.len());
                }
            }
            Box::pin(async {})
        }));
    }

    // Outbound: emulator → WebRTC DCs.
    //
    // Bridge drains all available UDP packets per wakeup (try_recv_from loop),
    // packs them per-peer into a single batch DC message, then deposits via a
    // watch channel (latest-value, never blocks). Each peer has an independent
    // send task so a slow dc.send() for one peer never stalls others.
    //
    // Batch wire format: [u16 len LE][data][u16 len LE][data]...
    let mut id_to_watch: HashMap<u8, tokio::sync::watch::Sender<Option<bytes::Bytes>>> =
        HashMap::new();
    for (&pid, dc) in &id_to_dc {
        let (tx, mut rx) = tokio::sync::watch::channel::<Option<bytes::Bytes>>(None);
        id_to_watch.insert(pid, tx);
        let dc2 = dc.clone();
        let log_send = log.clone();
        tokio::spawn(async move {
            loop {
                if rx.changed().await.is_err() { break; }
                let data = rx.borrow_and_update().clone();
                if let Some(data) = data {
                    let t0 = std::time::Instant::now();
                    let _ = dc2.send(&data).await;
                    let elapsed = t0.elapsed();
                    if elapsed.as_millis() >= 1 {
                        wlog!(log_send, "[out] pid={pid} dc.send() slow: {elapsed:?} {}B", data.len());
                    }
                }
            }
        });
    }

    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    *state.udp_listener_cancel.lock().unwrap() = Some(cancel_tx);
    let app2 = app.clone();
    let log_bridge = log.clone();
    tokio::spawn(async move {
        let bridge_state = app2.state::<AppState>();
        let mut buf = [0u8; 1 + 1400];
        let mut batches: HashMap<u8, bytes::BytesMut> = HashMap::new();
        tokio::pin!(cancel_rx);
        loop {
            // ── Blocking wait for the first packet of a burst ────────────────
            tokio::select! {
                biased;
                _ = &mut cancel_rx => break,
                result = udp_sock.recv_from(&mut buf) => {
                    match result {
                        Ok((len, addr)) if len >= 1 => {
                            if bridge_state.emulator_port.load(Ordering::Relaxed) == 0 {
                                bridge_state.emulator_port.store(addr.port(), Ordering::Relaxed);
                                wlog!(log_bridge, "[bridge] learned emulator port {}", addr.port());
                            }
                            let to_pid = buf[0];
                            let pkt = &buf[1..len];
                            let entry = batches.entry(to_pid).or_default();
                            entry.extend_from_slice(&(pkt.len() as u16).to_le_bytes());
                            entry.extend_from_slice(pkt);
                        }
                        Ok(_) => {} // zero-length wakeup
                        Err(e) => {
                            use std::io::ErrorKind;
                            match e.kind() {
                                ErrorKind::ConnectionReset | ErrorKind::ConnectionRefused => {}
                                _ => break,
                            }
                        }
                    }
                }
            }

            // ── Non-blocking drain: collect rest of burst ────────────────────
            let mut drain_count = 0u32;
            loop {
                match udp_sock.try_recv_from(&mut buf) {
                    Ok((len, _)) if len >= 1 => {
                        let to_pid = buf[0];
                        let pkt = &buf[1..len];
                        let entry = batches.entry(to_pid).or_default();
                        entry.extend_from_slice(&(pkt.len() as u16).to_le_bytes());
                        entry.extend_from_slice(pkt);
                        drain_count += 1;
                    }
                    _ => break,
                }
            }
            if drain_count > 2 {
                wlog!(log_bridge, "[bridge] large drain: +{drain_count} extra pkts");
            }

            // ── Flush one batch per peer via watch channel (instant) ─────────
            for (pid, batch) in batches.iter_mut() {
                if batch.is_empty() { continue; }
                if let Some(tx) = id_to_watch.get(pid) {
                    let data = batch.split().freeze();
                    let _ = tx.send(Some(data));
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
                let port = state.bridge_port.swap(0, Ordering::Relaxed);
                if port != 0 {
                    if let Ok(sock) = std::net::UdpSocket::bind("127.0.0.1:0") {
                        let _ = sock.send_to(&[], SocketAddr::from(([127, 0, 0, 1], port)));
                    }
                }
                state.emulator_port.store(0, Ordering::Relaxed);
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
    // Signal the watcher — it holds the child and will call start_kill().
    if let Some(tx) = state.game_watch_cancel.lock().unwrap().take() {
        let _ = tx.send(());
    }
    // Cancel the bridge recv thread: send the oneshot then kick the blocking
    // recv_from awake with a zero-length wakeup packet.
    if let Some(tx) = state.udp_listener_cancel.lock().unwrap().take() {
        let _ = tx.send(());
    }
    let port = state.bridge_port.swap(0, Ordering::Relaxed);
    if port != 0 {
        if let Ok(sock) = std::net::UdpSocket::bind("127.0.0.1:0") {
            let _ = sock.send_to(&[], SocketAddr::from(([127, 0, 0, 1], port)));
        }
    }
    // Zero the port so in-flight on_message closures become no-ops immediately.
    state.emulator_port.store(0, Ordering::Relaxed);
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

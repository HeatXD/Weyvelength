use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex as TokioMutex;
use webrtc::{
    api::{setting_engine::SettingEngine, APIBuilder},
    data_channel::{
        data_channel_init::RTCDataChannelInit,
        RTCDataChannel,
    },
    ice_transport::ice_server::RTCIceServer,
    peer_connection::policy::ice_transport_policy::RTCIceTransportPolicy,
    peer_connection::{
        configuration::RTCConfiguration,
        peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription,
    },
};

use crate::grpc::{
    weyvelength::{SendSignalRequest, Signal, SignalKind},
    WeyvelengthClient,
};
use crate::state::{AppState, IceServerEntry, PeerEntry};

// ── wire format ───────────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct PeerStatePayload {
    pub peer: String,
    /// One of: "checking" | "connected" | "open" | "disconnected" | "failed" | "closed"
    pub state: String,
}

#[derive(Serialize, Clone)]
pub struct MemberEventPayload {
    pub username: String,
    pub joined: bool,
}

// ── BoxError ──────────────────────────────────────────────────────────────────

type BoxError = Box<dyn std::error::Error + Send + Sync>;

// ── public entrypoint ─────────────────────────────────────────────────────────

/// Create a WebRTC peer connection and perform SDP negotiation.
/// Returns a `PeerEntry` whose `pc` and `dc` can be used to close the
/// connection and send data respectively.
pub async fn create_peer_connection(
    app: AppHandle,
    local_username: String,
    remote_username: String,
    session_id: String,
    ice_servers: Vec<IceServerEntry>,
    is_initiator: bool,
    initial_signal: Option<Signal>,
    force_relay: bool,
) -> Result<PeerEntry, BoxError> {
    // ── ICE configuration ─────────────────────────────────────────────────────
    let rtc_ice_servers: Vec<RTCIceServer> = if ice_servers.is_empty() {
        vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }]
    } else {
        ice_servers
            .iter()
            .map(|s| RTCIceServer {
                urls: vec![s.url.clone()],
                username: s.username.clone(),
                credential: s.credential.clone(),
                ..Default::default()
            })
            .collect()
    };

    let config = RTCConfiguration {
        ice_servers: rtc_ice_servers,
        ice_transport_policy: if force_relay {
            RTCIceTransportPolicy::Relay
        } else {
            RTCIceTransportPolicy::All
        },
        ..Default::default()
    };

    let mut se = SettingEngine::default();
    se.set_ice_timeouts(
        Some(Duration::from_secs(3)), // disconnected timeout
        Some(Duration::from_secs(5)), // failed timeout
        Some(Duration::from_secs(2)), // keepalive interval
    );
    let api = APIBuilder::new().with_setting_engine(se).build();
    let pc = Arc::new(api.new_peer_connection(config).await?);

    let game_dc_slot: Arc<TokioMutex<Option<Arc<RTCDataChannel>>>> =
        Arc::new(TokioMutex::new(None));

    // ── peer connection state callback ────────────────────────────────────────
    {
        let app2 = app.clone();
        let remote = remote_username.clone();
        pc.on_peer_connection_state_change(Box::new(
            move |state: RTCPeerConnectionState| {
                let app3 = app2.clone();
                let peer = remote.clone();
                let state_str = match state {
                    RTCPeerConnectionState::Connecting => "checking",
                    RTCPeerConnectionState::Connected => "connected",
                    RTCPeerConnectionState::Disconnected => "disconnected",
                    RTCPeerConnectionState::Failed => "failed",
                    RTCPeerConnectionState::Closed => "closed",
                    _ => "",
                }
                .to_string();
                Box::pin(async move {
                    eprintln!("[WebRTC] {peer}: {state:?}");
                    if !state_str.is_empty() {
                        let _ = app3.emit(
                            "peer-state",
                            PeerStatePayload { peer, state: state_str },
                        );
                    }
                })
            },
        ));
    }

    // ── ICE candidate callback (trickle ICE) ──────────────────────────────────
    {
        let app2 = app.clone();
        let local = local_username.clone();
        let remote = remote_username.clone();
        let sid = session_id.clone();
        pc.on_ice_candidate(Box::new(move |candidate| {
            let app3 = app2.clone();
            let local2 = local.clone();
            let remote2 = remote.clone();
            let sid2 = sid.clone();
            Box::pin(async move {
                if let Some(c) = candidate {
                    match c.to_json() {
                        Ok(init) => {
                            eprintln!("[ICE] candidate → {}", init.candidate);
                            match serde_json::to_string(&init) {
                            Ok(json) => {
                                let _ = send_signal_grpc(
                                    &app3,
                                    &local2,
                                    &remote2,
                                    &sid2,
                                    SignalKind::IceCandidate as i32,
                                    json,
                                )
                                .await;
                            }
                            Err(e) => eprintln!("[ICE] candidate serialize: {e}"),
                        }}
                        Err(e) => eprintln!("[ICE] candidate to_json: {e}"),
                    }
                }
            })
        }));
    }

    // ── SDP negotiation ───────────────────────────────────────────────────────

    // Unreliable + unordered — UDP-like, for future game-state traffic.
    let game_init = RTCDataChannelInit {
        ordered: Some(false),
        max_retransmits: Some(0),
        ..Default::default()
    };

    if is_initiator {
        // Create the game channel before the offer so it appears in the SDP.
        let game_dc = pc.create_data_channel("game", Some(game_init)).await?;
        register_game_dc_callbacks(&game_dc, &app, &remote_username);
        *game_dc_slot.lock().await = Some(game_dc);

        // Create offer and send.
        let offer = pc.create_offer(None).await?;
        pc.set_local_description(offer.clone()).await?;
        eprintln!("[ICE] sending offer to {remote_username}");
        send_signal_grpc(
            &app,
            &local_username,
            &remote_username,
            &session_id,
            SignalKind::SdpOffer as i32,
            offer.sdp,
        )
        .await?;
    } else {
        // Answerer: on_data_channel fires once per channel; route by label.
        let game_slot2 = game_dc_slot.clone();
        let app2 = app.clone();
        let remote = remote_username.clone();
        pc.on_data_channel(Box::new(move |dc| {
            let game_slot3 = game_slot2.clone();
            let app3 = app2.clone();
            let peer = remote.clone();
            Box::pin(async move {
                let label = dc.label().to_owned();
                eprintln!("[WebRTC] incoming data channel '{label}' from {peer}");
                if label == "game" {
                    register_game_dc_callbacks(&dc, &app3, &peer);
                    *game_slot3.lock().await = Some(dc);
                }
            })
        }));

        // Accept the offer and send answer.
        let offer_sdp = initial_signal
            .ok_or("answerer: no initial offer signal")?
            .payload;
        pc.set_remote_description(RTCSessionDescription::offer(offer_sdp)?).await?;
        let answer = pc.create_answer(None).await?;
        pc.set_local_description(answer.clone()).await?;
        eprintln!("[ICE] sending answer to {remote_username}");
        send_signal_grpc(
            &app,
            &local_username,
            &remote_username,
            &session_id,
            SignalKind::SdpAnswer as i32,
            answer.sdp,
        )
        .await?;
    }

    Ok(PeerEntry { pc, game_dc: game_dc_slot })
}

// ── data channel callback registration ───────────────────────────────────────

fn register_game_dc_callbacks(dc: &Arc<RTCDataChannel>, app: &AppHandle, remote: &str) {
    let peer = remote.to_owned();
    let app2 = app.clone();
    dc.on_open(Box::new(move || {
        let peer2 = peer.clone();
        let app3 = app2.clone();
        eprintln!("[WebRTC] game channel open with {peer2}");
        Box::pin(async move {
            let _ = app3.emit("peer-state", PeerStatePayload { peer: peer2, state: "open".to_string() });
        })
    }));

    let peer = remote.to_owned();
    dc.on_close(Box::new(move || {
        eprintln!("[WebRTC] game channel closed with {peer}");
        Box::pin(async {})
    }));
}

// ── gRPC signal helper ────────────────────────────────────────────────────────

pub(crate) async fn send_signal_grpc(
    app: &AppHandle,
    from: &str,
    to: &str,
    session_id: &str,
    kind: i32,
    payload: String,
) -> Result<(), BoxError> {
    let state = app.state::<AppState>();
    let channel = state.get_channel().map_err(|e| -> BoxError { e.into() })?;
    let mut client = WeyvelengthClient::new(channel);
    client
        .send_signal(SendSignalRequest {
            signal: Some(Signal {
                from_user: from.to_owned(),
                to_user: to.to_owned(),
                session_id: session_id.to_owned(),
                kind,
                payload,
            }),
        })
        .await
        .map_err(|e| -> BoxError { e.to_string().into() })?;
    Ok(())
}


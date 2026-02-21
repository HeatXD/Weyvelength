use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex as TokioMutex;
use webrtc::{
    api::APIBuilder,
    data_channel::{
        data_channel_init::RTCDataChannelInit,
        data_channel_message::DataChannelMessage as DcMsg,
        RTCDataChannel,
    },
    ice_transport::ice_server::RTCIceServer,
    peer_connection::{
        configuration::RTCConfiguration,
        peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription,
    },
};

use crate::commands::streaming::ChatMessagePayload;
use crate::grpc::{
    weyvelength::{SendSignalRequest, Signal, SignalKind},
    WeyvelengthClient,
};
use crate::state::{AppState, IceServerEntry, PeerEntry};

// ── wire format ───────────────────────────────────────────────────────────────

/// JSON envelope sent over the WebRTC data channel.
#[derive(Serialize, Deserialize)]
pub struct DataChannelMessage {
    pub username: String,
    pub content: String,
    pub timestamp: i64,
}

#[derive(Serialize, Clone)]
pub struct PeerStatePayload {
    pub peer: String,
    /// One of: "checking" | "connected" | "open" | "disconnected" | "closed"
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
        ..Default::default()
    };

    let api = APIBuilder::new().build();
    let pc = Arc::new(api.new_peer_connection(config).await?);

    let chat_dc_slot: Arc<TokioMutex<Option<Arc<RTCDataChannel>>>> =
        Arc::new(TokioMutex::new(None));
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
                    RTCPeerConnectionState::Failed => "disconnected",
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
                        Ok(init) => match serde_json::to_string(&init) {
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
                        },
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
        // Create both channels before the offer so they appear in the SDP.
        let chat_dc = pc.create_data_channel("chat", None).await?;
        register_chat_dc_callbacks(&chat_dc, &app, &remote_username);
        *chat_dc_slot.lock().await = Some(chat_dc);

        let game_dc = pc.create_data_channel("game", Some(game_init)).await?;
        register_game_dc_callbacks(&game_dc, &remote_username);
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
        let chat_slot2 = chat_dc_slot.clone();
        let game_slot2 = game_dc_slot.clone();
        let app2 = app.clone();
        let remote = remote_username.clone();
        pc.on_data_channel(Box::new(move |dc| {
            let chat_slot3 = chat_slot2.clone();
            let game_slot3 = game_slot2.clone();
            let app3 = app2.clone();
            let peer = remote.clone();
            Box::pin(async move {
                let label = dc.label().to_owned();
                eprintln!("[WebRTC] incoming data channel '{label}' from {peer}");
                if label == "chat" {
                    register_chat_dc_callbacks(&dc, &app3, &peer);
                    *chat_slot3.lock().await = Some(dc);
                } else if label == "game" {
                    register_game_dc_callbacks(&dc, &peer);
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

    Ok(PeerEntry { pc, chat_dc: chat_dc_slot, game_dc: game_dc_slot })
}

// ── data channel callback registration ───────────────────────────────────────

fn register_chat_dc_callbacks(dc: &Arc<RTCDataChannel>, app: &AppHandle, remote: &str) {
    // on_open — emit peer-state "open" so the UI knows the channel is ready.
    {
        let app2 = app.clone();
        let peer = remote.to_owned();
        dc.on_open(Box::new(move || {
            let app3 = app2.clone();
            let peer2 = peer.clone();
            Box::pin(async move {
                eprintln!("[WebRTC] chat channel open with {peer2}");
                let _ = app3.emit(
                    "peer-state",
                    PeerStatePayload { peer: peer2, state: "open".to_string() },
                );
            })
        }));
    }

    // on_message — deserialize JSON and emit "session-message".
    {
        let app2 = app.clone();
        dc.on_message(Box::new(move |msg: DcMsg| {
            let app3 = app2.clone();
            let data = msg.data.clone();
            Box::pin(async move {
                if let Ok(m) = serde_json::from_slice::<DataChannelMessage>(&data) {
                    let _ = app3.emit(
                        "session-message",
                        ChatMessagePayload {
                            username: m.username,
                            content: m.content,
                            timestamp: m.timestamp,
                        },
                    );
                }
            })
        }));
    }

    // on_close — emit peer-state "closed".
    {
        let app2 = app.clone();
        let peer = remote.to_owned();
        dc.on_close(Box::new(move || {
            let app3 = app2.clone();
            let peer2 = peer.clone();
            Box::pin(async move {
                let _ = app3.emit(
                    "peer-state",
                    PeerStatePayload { peer: peer2, state: "closed".to_string() },
                );
            })
        }));
    }
}

fn register_game_dc_callbacks(dc: &Arc<RTCDataChannel>, remote: &str) {
    let peer = remote.to_owned();
    dc.on_open(Box::new(move || {
        eprintln!("[WebRTC] game channel open with {peer}");
        Box::pin(async {})
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


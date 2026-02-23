use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::AtomicBool;

use tokio::sync::{oneshot, Mutex as TokioMutex};
use tonic::transport::Channel;

use webrtc::data_channel::RTCDataChannel;
use webrtc::peer_connection::RTCPeerConnection;

pub enum StreamKind {
    Global,
    Signals,
    SessionUpdates,
    GlobalMembers,
    SessionMessages,
}

#[derive(Clone)]
pub struct IceServerEntry {
    pub url: String,
    pub username: String,
    pub credential: String,
    pub name: String,
}

/// Per-peer state. Call pc.close().await to shut down cleanly.
pub struct PeerEntry {
    pub pc: Arc<RTCPeerConnection>,
    /// Unreliable, unordered — reserved for the UDP-proxy game bridge.
    #[allow(dead_code)]
    pub game_dc: Arc<TokioMutex<Option<Arc<RTCDataChannel>>>>,
}

pub struct AppState {
    pub channel: RwLock<Option<Channel>>,
    pub username: RwLock<Option<String>>,
    pub global_cancel_tx: Mutex<Option<oneshot::Sender<()>>>,
    pub signals_cancel_tx: Mutex<Option<oneshot::Sender<()>>>,
    pub session_updates_cancel_tx: Mutex<Option<oneshot::Sender<()>>>,
    pub global_members_cancel_tx: Mutex<Option<oneshot::Sender<()>>>,
    pub session_messages_cancel_tx: Mutex<Option<oneshot::Sender<()>>>,
    pub ice_servers: RwLock<Vec<IceServerEntry>>,
    /// Name of the TURN server the user has selected (None = skip TURN, direct only).
    pub selected_turn: RwLock<Option<String>>,
    pub peer_connections: Mutex<HashMap<String, PeerEntry>>,
    pub current_session_id: Mutex<Option<String>>,
    /// When true, WebRTC is configured to use only TURN relay candidates.
    pub force_relay: AtomicBool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            channel: RwLock::new(None),
            username: RwLock::new(None),
            global_cancel_tx: Mutex::new(None),
            signals_cancel_tx: Mutex::new(None),
            session_updates_cancel_tx: Mutex::new(None),
            global_members_cancel_tx: Mutex::new(None),
            session_messages_cancel_tx: Mutex::new(None),
            ice_servers: RwLock::new(Vec::new()),
            selected_turn: RwLock::new(None),
            peer_connections: Mutex::new(HashMap::new()),
            current_session_id: Mutex::new(None),
            force_relay: AtomicBool::new(false),
        }
    }

    pub fn get_channel(&self) -> Result<Channel, String> {
        self.channel
            .read()
            .unwrap()
            .clone()
            .ok_or_else(|| "Not connected".into())
    }

    pub fn get_username(&self) -> Result<String, String> {
        self.username
            .read()
            .unwrap()
            .clone()
            .ok_or_else(|| "No username set".into())
    }

    /// Look up a peer connection by username. Locks briefly; does not hold across awaits.
    pub fn get_peer_connection(&self, peer: &str) -> Option<Arc<RTCPeerConnection>> {
        self.peer_connections
            .lock()
            .unwrap()
            .get(peer)
            .map(|e| e.pc.clone())
    }

    fn kind_mutex(&self, kind: &StreamKind) -> &Mutex<Option<oneshot::Sender<()>>> {
        match kind {
            StreamKind::Global => &self.global_cancel_tx,
            StreamKind::Signals => &self.signals_cancel_tx,
            StreamKind::SessionUpdates => &self.session_updates_cancel_tx,
            StreamKind::GlobalMembers => &self.global_members_cancel_tx,
            StreamKind::SessionMessages => &self.session_messages_cancel_tx,
        }
    }

    pub fn reset_stream(&self, kind: StreamKind) -> oneshot::Receiver<()> {
        let mutex = self.kind_mutex(&kind);
        if let Some(tx) = mutex.lock().unwrap().take() {
            let _ = tx.send(());
        }
        let (tx, rx) = oneshot::channel();
        *mutex.lock().unwrap() = Some(tx);
        rx
    }

    pub fn cancel_stream(&self, kind: StreamKind) {
        let mutex = self.kind_mutex(&kind);
        if let Some(tx) = mutex.lock().unwrap().take() {
            let _ = tx.send(());
        }
    }

    pub fn cancel_all_streams(&self) {
        for kind in [
            StreamKind::Global,
            StreamKind::Signals,
            StreamKind::SessionUpdates,
            StreamKind::GlobalMembers,
            StreamKind::SessionMessages,
        ] {
            self.cancel_stream(kind);
        }
    }

    /// Returns the effective ICE server list: all STUN servers plus the
    /// user-selected TURN server (if any). Mirrors the filter in join_session_webrtc.
    pub fn effective_ice_servers(&self) -> Vec<IceServerEntry> {
        let all = self.ice_servers.read().unwrap().clone();
        let selected_turn = self.selected_turn.read().unwrap().clone();
        all.into_iter()
            .filter(|s| {
                if s.url.starts_with("stun:") || s.url.starts_with("stuns:") {
                    return true;
                }
                selected_turn.as_deref() == Some(s.name.as_str()) && !s.name.is_empty()
            })
            .collect()
    }

    /// Close all peer connections gracefully, calling pc.close().await on each.
    pub async fn close_all_peer_connections(&self) {
        let entries: Vec<Arc<RTCPeerConnection>> = {
            self.peer_connections
                .lock()
                .unwrap()
                .drain()
                .map(|(_, e)| e.pc)
                .collect()
        };
        for pc in entries {
            let _ = pc.close().await;
        }
    }
}

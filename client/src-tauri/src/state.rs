use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::{oneshot, Mutex as TokioMutex};
use tonic::transport::Channel;

use webrtc::data_channel::RTCDataChannel;
use webrtc::peer_connection::RTCPeerConnection;

pub enum StreamKind {
    Global,
    Signals,
    SessionUpdates,
    GlobalMembers,
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
    /// Reliable, ordered — chat messages (JSON).
    pub chat_dc: Arc<TokioMutex<Option<Arc<RTCDataChannel>>>>,
    /// Unreliable, unordered — reserved for the UDP-proxy game bridge.
    #[allow(dead_code)]
    pub game_dc: Arc<TokioMutex<Option<Arc<RTCDataChannel>>>>,
}

pub struct AppState {
    pub channel: Mutex<Option<Channel>>,
    pub username: Mutex<Option<String>>,
    pub global_cancel_tx: Mutex<Option<oneshot::Sender<()>>>,
    pub signals_cancel_tx: Mutex<Option<oneshot::Sender<()>>>,
    pub session_updates_cancel_tx: Mutex<Option<oneshot::Sender<()>>>,
    pub global_members_cancel_tx: Mutex<Option<oneshot::Sender<()>>>,
    pub ice_servers: Mutex<Vec<IceServerEntry>>,
    /// Name of the TURN server the user has selected (None = skip TURN, direct only).
    pub selected_turn: Mutex<Option<String>>,
    pub peer_connections: Mutex<HashMap<String, PeerEntry>>,
    pub current_session_id: Mutex<Option<String>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            channel: Mutex::new(None),
            username: Mutex::new(None),
            global_cancel_tx: Mutex::new(None),
            signals_cancel_tx: Mutex::new(None),
            session_updates_cancel_tx: Mutex::new(None),
            global_members_cancel_tx: Mutex::new(None),
            ice_servers: Mutex::new(Vec::new()),
            selected_turn: Mutex::new(None),
            peer_connections: Mutex::new(HashMap::new()),
            current_session_id: Mutex::new(None),
        }
    }

    pub fn get_channel(&self) -> Result<Channel, String> {
        self.channel
            .lock()
            .map_err(|_| "Channel lock poisoned".to_string())?
            .clone()
            .ok_or_else(|| "Not connected".into())
    }

    pub fn get_username(&self) -> Result<String, String> {
        self.username
            .lock()
            .map_err(|_| "Username lock poisoned".to_string())?
            .clone()
            .ok_or_else(|| "No username set".into())
    }

    fn kind_mutex(&self, kind: &StreamKind) -> &Mutex<Option<oneshot::Sender<()>>> {
        match kind {
            StreamKind::Global => &self.global_cancel_tx,
            StreamKind::Signals => &self.signals_cancel_tx,
            StreamKind::SessionUpdates => &self.session_updates_cancel_tx,
            StreamKind::GlobalMembers => &self.global_members_cancel_tx,
        }
    }

    pub fn reset_stream(&self, kind: StreamKind) -> oneshot::Receiver<()> {
        let mutex = self.kind_mutex(&kind);
        if let Some(tx) = mutex.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = tx.send(());
        }
        let (tx, rx) = oneshot::channel();
        *mutex.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
        rx
    }

    pub fn cancel_stream(&self, kind: StreamKind) {
        let mutex = self.kind_mutex(&kind);
        if let Some(tx) = mutex.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = tx.send(());
        }
    }

    pub fn cancel_all_streams(&self) {
        for kind in [
            StreamKind::Global,
            StreamKind::Signals,
            StreamKind::SessionUpdates,
            StreamKind::GlobalMembers,
        ] {
            self.cancel_stream(kind);
        }
    }

    /// Returns the effective ICE server list: all STUN servers plus the
    /// user-selected TURN server (if any). Mirrors the filter in join_session_webrtc.
    pub fn effective_ice_servers(&self) -> Vec<IceServerEntry> {
        let all = self.ice_servers.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let selected_turn = self.selected_turn.lock().unwrap_or_else(|e| e.into_inner()).clone();
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
                .unwrap_or_else(|e| e.into_inner())
                .drain()
                .map(|(_, e)| e.pc)
                .collect()
        };
        for pc in entries {
            let _ = pc.close().await;
        }
    }
}

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc};

use crate::codegen::weyvelength::{ChatMessage, Signal};

pub const GLOBAL_SESSION_ID: &str = "__global__";
pub const BROADCAST_CAPACITY: usize = 256;
pub const SESSION_MIN_MEMBERS: u32 = 2;
pub const SESSION_MAX_MEMBERS: u32 = 16;

pub struct IceServerConfig {
    pub url: String,
    pub username: String,
    pub credential: String,
    pub name: String,
}

/// Mutable per-session state — protected by a tokio-aware Mutex so sessions
/// don't contend with each other and the executor thread is never blocked.
pub struct SessionInner {
    pub host: String,
    pub members: HashSet<String>,
    pub signal_senders: HashMap<String, mpsc::UnboundedSender<Arc<Signal>>>,
}

/// Per-session data. Immutable fields are readable without a lock;
/// mutable state lives in `inner`.
pub struct SessionData {
    pub id: String,
    pub name: String,
    pub is_public: bool,
    pub max_members: u32,
    /// Broadcast channel for chat messages — cloning the Sender is cheap and lock-free.
    pub tx: broadcast::Sender<ChatMessage>,
    pub inner: tokio::sync::Mutex<SessionInner>,
}

pub struct ServerState {
    // Immutable after initialisation — no lock needed.
    pub server_name: String,
    pub motd: String,
    pub ice_servers: Vec<IceServerConfig>,

    /// Session registry. DashMap: concurrent access to different sessions never
    /// contends. Write (insert/remove) takes a shard write-lock briefly.
    pub sessions: DashMap<String, Arc<SessionData>>,

    /// Maps username → session_id. DashMap; join/leave are the only writers.
    /// Acts as the authoritative ownership record for join/leave races.
    pub user_session_index: DashMap<String, String>,

    /// Fires whenever the public session list changes.
    pub session_update_tx: broadcast::Sender<()>,

    /// Fires whenever the global (online) member list changes.
    pub global_members_tx: broadcast::Sender<()>,

    /// Counts active `StreamMessages` connections per username for the global session.
    /// A user is removed from `global.members` only when this reaches zero.
    /// DashMap: increment/decrement are synchronous — no async lock needed.
    pub global_stream_refs: DashMap<String, usize>,
}

/// `Arc<ServerState>` — interior mutability is per-field, no outer Mutex.
pub type SharedState = Arc<ServerState>;

impl ServerState {
    pub fn new(server_name: String, motd: String, ice_servers: Vec<IceServerConfig>) -> Self {
        let (chat_tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        let (session_update_tx, _) = broadcast::channel(16);
        let (global_members_tx, _) = broadcast::channel(16);

        let global = Arc::new(SessionData {
            id: GLOBAL_SESSION_ID.to_string(),
            name: "global".into(),
            is_public: false,
            max_members: 0,
            tx: chat_tx,
            inner: tokio::sync::Mutex::new(SessionInner {
                host: String::new(),
                members: HashSet::new(),
                signal_senders: HashMap::new(),
            }),
        });

        let sessions = DashMap::new();
        sessions.insert(GLOBAL_SESSION_ID.to_string(), global);

        Self {
            server_name,
            motd,
            ice_servers,
            sessions,
            user_session_index: DashMap::new(),
            session_update_tx,
            global_members_tx,
            global_stream_refs: DashMap::new(),
        }
    }
}

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, mpsc};

use crate::codegen::weyvelength::{ChatMessage, Signal};

pub const GLOBAL_SESSION_ID: &str = "__global__";
pub const BROADCAST_CAPACITY: usize = 256;

pub struct IceServerConfig {
    pub url: String,
    pub username: String,
    pub credential: String,
    pub name: String,
}

pub struct SessionData {
    pub id: String,
    pub name: String,
    pub host: String,
    pub members: Vec<String>,
    pub is_public: bool,
    pub max_members: u32, // 0 = unlimited
    pub tx: broadcast::Sender<ChatMessage>,
    pub signal_senders: HashMap<String, mpsc::UnboundedSender<Signal>>,
}

pub struct ServerState {
    pub server_name: String,
    pub motd: String,
    pub sessions: HashMap<String, SessionData>,
    pub user_session_index: HashMap<String, String>,
    /// Fires `()` whenever the public session list changes (created, removed, membership changed).
    pub session_update_tx: broadcast::Sender<()>,
    /// Fires `()` whenever the global (online) member list changes.
    pub global_members_tx: broadcast::Sender<()>,
    /// Counts active `stream_messages` connections per username for the global session.
    /// A user is removed from `global.members` only when this reaches zero.
    pub global_stream_refs: HashMap<String, usize>,
    pub ice_servers: Vec<IceServerConfig>,
}

impl ServerState {
    pub fn new(server_name: String, motd: String, ice_servers: Vec<IceServerConfig>) -> Self {
        let (chat_tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        let (session_update_tx, _) = broadcast::channel(16);
        let (global_members_tx, _) = broadcast::channel(16);
        let mut sessions = HashMap::new();
        sessions.insert(
            GLOBAL_SESSION_ID.to_string(),
            SessionData {
                id: GLOBAL_SESSION_ID.to_string(),
                name: "global".into(),
                host: String::new(),
                members: vec![],
                is_public: false,
                max_members: 0,
                tx: chat_tx,
                signal_senders: HashMap::new(),
            },
        );
        Self {
            server_name,
            motd,
            sessions,
            user_session_index: HashMap::new(),
            session_update_tx,
            global_members_tx,
            global_stream_refs: HashMap::new(),
            ice_servers,
        }
    }
}

pub type SharedState = Arc<Mutex<ServerState>>;

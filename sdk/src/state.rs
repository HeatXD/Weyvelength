// Internal types, globals, and helpers, not part of the public C API.

use std::ffi::{CString, c_int, c_void};
use std::sync::{Mutex, OnceLock};

// ── Constants ─────────────────────────────────────────────────────────────────

pub const WL_PACKET_MAX: usize = 1400;

// ── Receive callback ──────────────────────────────────────────────────────────
//
// Called from the background IO thread the instant a packet arrives.
// `data` points into the SDK's receive buffer — valid only for the duration
// of the callback. wl_send() is safe to call from inside the callback.

pub type RecvCallbackFn = unsafe extern "C" fn(
    from_player_id: u8,
    data: *const c_void,
    data_len: c_int,
    userdata: *mut c_void,
);

#[derive(Copy, Clone)]
pub struct RecvCallback {
    pub func: RecvCallbackFn,
    pub userdata: *mut c_void,
}

// Safety: the caller promises func and userdata are safe to use from any thread.
unsafe impl Send for RecvCallback {}
unsafe impl Sync for RecvCallback {}

// ── SDK state ─────────────────────────────────────────────────────────────────

pub struct SdkState {
    pub rt: tokio::runtime::Runtime,
    pub local_player_id: u8,
    /// Game pushes outbound frames here — lock-free, never blocks.
    pub outbound_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    /// Drop both to signal both tasks to stop.
    pub shutdown_tx: Option<(
        tokio::sync::oneshot::Sender<()>,
        tokio::sync::oneshot::Sender<()>,
    )>,
}

// ── Globals ───────────────────────────────────────────────────────────────────

pub static GLOBAL: Mutex<Option<SdkState>> = Mutex::new(None);
pub static CALLBACK: Mutex<Option<RecvCallback>> = Mutex::new(None);

static ERROR: OnceLock<Mutex<CString>> = OnceLock::new();

pub fn error_lock() -> &'static Mutex<CString> {
    ERROR.get_or_init(|| Mutex::new(CString::new("OK").unwrap()))
}

pub fn set_error(msg: impl AsRef<str>) {
    let mut lock = error_lock().lock().unwrap();
    *lock = CString::new(msg.as_ref()).unwrap_or_else(|_| CString::new("error").unwrap());
}

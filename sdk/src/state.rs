// Internal types, globals, and helpers — not part of the public C API.

use std::ffi::CString;
use std::sync::{Mutex, OnceLock};

use tokio::sync::{mpsc, oneshot};

// ── Constants ─────────────────────────────────────────────────────────────────

pub const WL_PACKET_MAX: usize = 1400;

// ── Public C-compatible event struct ──────────────────────────────────────────
//
// Must exactly mirror WL_Event in wl-sdk.h.
// C layout: kind(u32,4) + from_player_id(u8,1) + _pad([u8;3],3)
//           + data_len(i32,4) + data([u8;1400],1400)  = 1412 bytes total

#[repr(C)]
pub struct WlEvent {
    pub kind:           u32,
    pub from_player_id: u8,
    pub _pad:           [u8; 3],
    pub data_len:       i32,
    pub data:           [u8; WL_PACKET_MAX],
}

// ── SDK state ─────────────────────────────────────────────────────────────────

pub struct SdkState {
    /// Inbound events pushed by run_io; drained synchronously by wl_poll.
    pub event_rx:        mpsc::UnboundedReceiver<WlEvent>,
    /// Outbound frames pulled by run_io; pushed synchronously by wl_send.
    pub send_tx:         mpsc::UnboundedSender<Vec<u8>>,
    /// Dropping this field signals run_io to exit.
    pub _shutdown_tx:    oneshot::Sender<()>,
    pub local_player_id: u8,
    /// Runtime dropped last (field order) — blocks until run_io finishes.
    pub _runtime:        tokio::runtime::Runtime,
}

// ── Globals ───────────────────────────────────────────────────────────────────

pub static GLOBAL:   Mutex<Option<SdkState>> = Mutex::new(None);
pub static POLL_BUF: Mutex<Vec<WlEvent>>     = Mutex::new(Vec::new());

static ERROR: OnceLock<Mutex<CString>> = OnceLock::new();

pub fn error_lock() -> &'static Mutex<CString> {
    ERROR.get_or_init(|| Mutex::new(CString::new("OK").unwrap()))
}

pub fn set_error(msg: impl AsRef<str>) {
    let mut lock = error_lock().lock().unwrap();
    *lock = CString::new(msg.as_ref()).unwrap_or_else(|_| CString::new("error").unwrap());
}

// ── Layout verification ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wl_event_layout() {
        assert_eq!(std::mem::size_of::<WlEvent>(), 1412);
        assert_eq!(std::mem::offset_of!(WlEvent, kind),           0);
        assert_eq!(std::mem::offset_of!(WlEvent, from_player_id), 4);
        assert_eq!(std::mem::offset_of!(WlEvent, data_len),       8);
        assert_eq!(std::mem::offset_of!(WlEvent, data),           12);
    }
}

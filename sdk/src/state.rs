// Internal types and globals — not part of the public C API.

use std::collections::VecDeque;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

pub const WL_PACKET_MAX: usize = 1400;
/// Maximum queued inbound packets before the oldest is discarded.
pub const QUEUE_CAP: usize = 256;

// ── Globals ───────────────────────────────────────────────────────────────────
//
// RECV_SOCK is Arc<UdpSocket> so the background recv thread holds a clone
// without keeping the mutex locked during blocking recv_from.
// SEND_SOCK is a separate try_clone()'d handle — wl_send() never aliases recv.

pub static RECV_SOCK: Mutex<Option<Arc<UdpSocket>>> = Mutex::new(None);
pub static SEND_SOCK: Mutex<Option<(UdpSocket, SocketAddr)>> = Mutex::new(None);
pub static PLAYER_ID: AtomicU8 = AtomicU8::new(0);
/// Local port of the recv socket; used to send a wakeup packet on shutdown.
pub static LOCAL_PORT: AtomicU16 = AtomicU16::new(0);
/// Per-session stop flag owned by the background recv thread.
pub static STOP_ARC: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);
/// Inbound packet queue. Background thread pushes; wl_recv pops.
pub static RECV_QUEUE: Mutex<VecDeque<(u8, Box<[u8]>)>> = Mutex::new(VecDeque::new());

// ERROR is written immediately before returning from a failing call and read
// immediately after by the caller. Raw pointer access avoids holding a lock
// across the C boundary while keeping the error path lock-free.
pub static mut ERROR: [u8; 128] = [0u8; 128];

pub fn set_error(msg: &str) {
    let src = msg.as_bytes();
    let len = src.len().min(127);
    // SAFETY: set_error is only called by the thread that just made an API call;
    // no other thread writes ERROR concurrently.
    unsafe {
        let ptr = std::ptr::addr_of_mut!(ERROR) as *mut u8;
        std::ptr::copy_nonoverlapping(src.as_ptr(), ptr, len);
        *ptr.add(len) = 0;
    }
}

pub fn store_player_id(id: u8) {
    PLAYER_ID.store(id, Ordering::Relaxed);
}

pub fn load_player_id() -> u8 {
    PLAYER_ID.load(Ordering::Relaxed)
}

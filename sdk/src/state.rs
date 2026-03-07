// Internal types and globals — not part of the public C API.

use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

pub const WL_PACKET_MAX: usize = 1400;

// ── Globals ───────────────────────────────────────────────────────────────────
//
// RECV_SOCK and SEND_SOCK are try_clone()'d handles to the same OS socket.
// Keeping them separate means wl_send() never aliases wl_recv(), even when
// wl_send() is called while wl_recv() holds the RECV_SOCK lock.

pub static RECV_SOCK: Mutex<Option<UdpSocket>> = Mutex::new(None);
pub static SEND_SOCK: Mutex<Option<(UdpSocket, SocketAddr)>> = Mutex::new(None);
pub static PLAYER_ID: AtomicU8 = AtomicU8::new(0);

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

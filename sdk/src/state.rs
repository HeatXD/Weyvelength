// Internal types and globals — not part of the public C API.

use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

pub const WL_PACKET_MAX: usize = 1400;

// ── Globals ───────────────────────────────────────────────────────────────────
//
// RECV_SOCK and SEND_SOCK are try_clone() handles of the same underlying OS
// socket, so they share the same local port. The bridge discovers the SDK's
// port from the source address of the first outbound packet and sends all
// inbound data back to that port.
//
// The socket is set non-blocking so wl_recv returns immediately (None) when
// no packet is available, matching ASIO adapter behaviour. The OS recv buffer
// (256 KB via SO_RCVBUF) absorbs bursts between game-tick polls.
// No intermediate software queue: window-hold freezes stop polling, so stale
// packets are naturally aged out by the OS buffer rather than accumulating.

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

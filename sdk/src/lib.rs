//! Weyvelength SDK, Rust implementation with C-compatible API.
//!
//! A tokio background task continuously drains the UDP bridge socket into an
//! internal queue. `wl_poll` snapshots that queue; the game loop never blocks
//! on I/O. `wl_send` routes outbound packets to the WL client bridge via the
//! same socket.
//!
//! All exported functions are single-threaded from the caller's perspective.

mod api;
mod core;
mod state;

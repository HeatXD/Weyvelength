//! Weyvelength SDK — pure-std, pull-based C API.
//!
//! 1. Call `wl_init()` with the bridge port and player ID from the Weyvelength client.
//! 2. Each game tick, call `wl_recv()` in a loop until it returns 0 to drain all
//!    pending inbound packets.
//! 3. Call `wl_send()` to transmit to a peer immediately.
//! 4. Call `wl_shutdown()` on exit.

mod api;
mod core;
mod state;

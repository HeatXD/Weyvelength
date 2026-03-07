// All SDK logic. No heap allocation in the hot path.

use std::net::{SocketAddr, UdpSocket};

use crate::state::{RECV_SOCK, SEND_SOCK, WL_PACKET_MAX, load_player_id, store_player_id};

pub fn init(bridge_port: i32, local_player_id: u8) -> Result<(), String> {
    shutdown();

    let bridge_addr: SocketAddr = format!("127.0.0.1:{bridge_port}")
        .parse()
        .map_err(|_| format!("invalid bridge port: {bridge_port}"))?;

    let recv = UdpSocket::bind("127.0.0.1:0").map_err(|e| format!("bind: {e}"))?;
    recv.set_nonblocking(true).map_err(|e| format!("set_nonblocking: {e}"))?;
    let send = recv.try_clone().map_err(|e| format!("clone socket: {e}"))?;

    *RECV_SOCK.lock().unwrap() = Some(recv);
    *SEND_SOCK.lock().unwrap() = Some((send, bridge_addr));
    store_player_id(local_player_id);

    Ok(())
}

/// Receives one pending inbound packet into `buf`.
/// Returns `Some((from_player_id, data_len))` or `None` if no packet is available.
/// Non-blocking — call in a loop until `None` to drain all pending packets.
pub fn recv(buf: &mut [u8]) -> Option<(u8, usize)> {
    let guard = RECV_SOCK.lock().unwrap();
    let Some(sock) = guard.as_ref() else { return None };

    let mut frame = [0u8; 1 + WL_PACKET_MAX];
    match sock.recv_from(&mut frame) {
        Ok((n, _)) if n >= 1 => {
            let data_len = (n - 1).min(buf.len());
            buf[..data_len].copy_from_slice(&frame[1..1 + data_len]);
            Some((frame[0], data_len))
        }
        _ => None,
    }
}

pub fn shutdown() {
    *RECV_SOCK.lock().unwrap() = None;
    *SEND_SOCK.lock().unwrap() = None;
    store_player_id(0);
}

pub fn local_player_id() -> u8 {
    load_player_id()
}

/// Sends immediately. Stack-allocated frame — no heap allocation.
/// Outbound frame: [u8 to_player_id][data...]
pub fn send(to_player_id: u8, payload: &[u8]) -> Result<(), String> {
    let guard = SEND_SOCK.lock().unwrap();
    let Some((sock, addr)) = guard.as_ref() else {
        return Err("not initialised".to_owned());
    };

    let len = payload.len().min(WL_PACKET_MAX);
    let mut frame = [0u8; 1 + WL_PACKET_MAX];
    frame[0] = to_player_id;
    frame[1..1 + len].copy_from_slice(&payload[..len]);

    sock.send_to(&frame[..1 + len], *addr)
        .map_err(|e| format!("send_to: {e}"))?;
    Ok(())
}

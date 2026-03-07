// All SDK logic.

use std::net::{SocketAddr, UdpSocket};

use crate::state::{
    RECV_SOCK, SEND_SOCK, WL_PACKET_MAX,
    load_player_id, store_player_id,
};

pub fn init(bridge_port: i32, local_player_id: u8) -> Result<(), String> {
    shutdown();

    let bridge_addr: SocketAddr = format!("127.0.0.1:{bridge_port}")
        .parse()
        .map_err(|_| format!("invalid bridge port: {bridge_port}"))?;

    // Single socket, non-blocking. try_clone shares the same OS socket and
    // local port for both recv and send. The bridge discovers the SDK's port
    // from the source address of the first outbound packet.
    // SO_RCVBUF 256 KB absorbs bursts between game-tick polls.
    let sock = UdpSocket::bind("127.0.0.1:0").map_err(|e| format!("bind: {e}"))?;
    let sock = set_rcvbuf(sock);
    sock.set_nonblocking(true).map_err(|e| format!("set_nonblocking: {e}"))?;
    let send = sock.try_clone().map_err(|e| format!("clone socket: {e}"))?;

    *RECV_SOCK.lock().unwrap() = Some(sock);
    *SEND_SOCK.lock().unwrap() = Some((send, bridge_addr));
    store_player_id(local_player_id);

    Ok(())
}

/// Pops one inbound packet from the OS recv buffer into `buf`.
/// Returns `Some((from_player_id, data_len))` or `None` if no packet is waiting.
/// Call in a loop each tick until it returns `None`.
pub fn recv(buf: &mut [u8]) -> Option<(u8, usize)> {
    let guard = RECV_SOCK.lock().unwrap();
    let sock = guard.as_ref()?;
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

/// Sends immediately. Frame built before acquiring the lock to minimise hold time.
/// Outbound wire format: [u8 to_player_id][data...]
pub fn send(to_player_id: u8, payload: &[u8]) -> Result<(), String> {
    let len = payload.len().min(WL_PACKET_MAX);
    let mut frame = [0u8; 1 + WL_PACKET_MAX];
    frame[0] = to_player_id;
    frame[1..1 + len].copy_from_slice(&payload[..len]);

    let guard = SEND_SOCK.lock().unwrap();
    let Some((sock, addr)) = guard.as_ref() else {
        return Err("not initialised".to_owned());
    };
    sock.send_to(&frame[..1 + len], *addr)
        .map_err(|e| format!("send_to: {e}"))?;
    Ok(())
}

fn set_rcvbuf(sock: UdpSocket) -> UdpSocket {
    let s: socket2::Socket = sock.into();
    let _ = s.set_recv_buffer_size(256 * 1024);
    s.into()
}

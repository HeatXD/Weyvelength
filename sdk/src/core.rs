// All SDK logic.

use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::state::{
    LOCAL_PORT, QUEUE_CAP, RECV_QUEUE, RECV_SOCK, SEND_SOCK, STOP_ARC, WL_PACKET_MAX,
    load_player_id, store_player_id,
};

pub fn init(bridge_port: i32, local_player_id: u8) -> Result<(), String> {
    shutdown();

    let bridge_addr: SocketAddr = format!("127.0.0.1:{bridge_port}")
        .parse()
        .map_err(|_| format!("invalid bridge port: {bridge_port}"))?;

    // Blocking socket — background thread wakes immediately on each packet.
    let recv = UdpSocket::bind("127.0.0.1:0").map_err(|e| format!("bind: {e}"))?;
    let local_port = recv.local_addr().unwrap().port();
    let send = recv.try_clone().map_err(|e| format!("clone socket: {e}"))?;

    let recv_arc = Arc::new(recv);
    *RECV_SOCK.lock().unwrap() = Some(recv_arc.clone());
    *SEND_SOCK.lock().unwrap() = Some((send, bridge_addr));
    LOCAL_PORT.store(local_port, Ordering::Relaxed);
    store_player_id(local_player_id);

    let stop = Arc::new(AtomicBool::new(false));
    *STOP_ARC.lock().unwrap() = Some(stop.clone());

    std::thread::spawn(move || {
        let mut frame = [0u8; 1 + WL_PACKET_MAX];
        loop {
            match recv_arc.recv_from(&mut frame) {
                Ok((n, _)) if n >= 1 => {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let from_id = frame[0];
                    let data_len = n - 1;
                    let mut q = RECV_QUEUE.lock().unwrap();
                    if q.len() >= QUEUE_CAP {
                        q.pop_front(); // discard oldest
                    }
                    q.push_back((from_id, frame[1..1 + data_len].to_vec().into_boxed_slice()));
                }
                Ok(_) => {
                    // zero-length wakeup packet — check stop flag
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok(())
}

/// Pops one inbound packet from the queue into `buf`.
/// Returns `Some((from_player_id, data_len))` or `None` if the queue is empty.
pub fn recv(buf: &mut [u8]) -> Option<(u8, usize)> {
    let (from_id, data) = RECV_QUEUE.lock().unwrap().pop_front()?;
    let len = data.len().min(buf.len());
    buf[..len].copy_from_slice(&data[..len]);
    Some((from_id, len))
}

pub fn shutdown() {
    // Signal the background thread to stop.
    if let Some(stop) = STOP_ARC.lock().unwrap().take() {
        stop.store(true, Ordering::Relaxed);
    }
    // Unblock the background thread's blocking recv_from with a wakeup packet.
    let port = LOCAL_PORT.swap(0, Ordering::Relaxed);
    if port != 0 {
        if let Ok(sock) = UdpSocket::bind("127.0.0.1:0") {
            let _ = sock.send_to(&[0u8], SocketAddr::from(([127, 0, 0, 1], port)));
        }
    }
    *RECV_SOCK.lock().unwrap() = None;
    *SEND_SOCK.lock().unwrap() = None;
    RECV_QUEUE.lock().unwrap().clear();
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

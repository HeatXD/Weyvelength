// All SDK implementation logic. No unsafe, no C types.

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::UdpSocket;
use tokio::sync::{mpsc, oneshot};

use crate::state::{SdkState, WlEvent, WL_PACKET_MAX, GLOBAL, POLL_BUF};

pub fn init(bridge_port: i32, local_player_id: u8) -> Result<(), String> {
    shutdown();

    let bridge_addr: SocketAddr = format!("127.0.0.1:{bridge_port}").parse().unwrap();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_io()
        .build()
        .map_err(|e| format!("runtime error: {e}"))?;

    let socket = runtime
        .block_on(UdpSocket::bind("127.0.0.1:0"))
        .map(Arc::new)
        .map_err(|e| format!("bind error: {e}"))?;

    let (event_tx, event_rx) = mpsc::unbounded_channel::<WlEvent>();
    let (send_tx, send_rx)   = mpsc::unbounded_channel::<Vec<u8>>();
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    runtime.spawn(run_io(socket, event_tx, send_rx, bridge_addr, shutdown_rx));

    *GLOBAL.lock().unwrap() = Some(SdkState {
        event_rx,
        send_tx,
        _shutdown_tx:    shutdown_tx,
        local_player_id,
        _runtime:        runtime,
    });

    Ok(())
}

pub fn shutdown() {
    // _shutdown_tx drops first → run_io receives cancellation.
    // _runtime drops last → blocks until run_io exits.
    drop(GLOBAL.lock().unwrap().take());
    POLL_BUF.lock().unwrap().clear();
}

pub fn local_player_id() -> u8 {
    GLOBAL.lock().unwrap().as_ref().map_or(0, |s| s.local_player_id)
}

/// Drains all queued inbound events. Fully synchronous — no block_on.
pub fn poll() -> (*const WlEvent, i32) {
    let mut lock = GLOBAL.lock().unwrap();
    let Some(state) = lock.as_mut() else { return (std::ptr::null(), 0); };

    let mut buf = POLL_BUF.lock().unwrap();
    buf.clear();
    while let Ok(ev) = state.event_rx.try_recv() {
        buf.push(ev);
    }
    let count = buf.len() as i32;
    let ptr = if buf.is_empty() { std::ptr::null() } else { buf.as_ptr() };
    (ptr, count)
}

/// Queues a frame for delivery. Fully synchronous — no block_on.
pub fn send(to_player_id: u8, payload: &[u8]) -> Result<(), String> {
    let lock = GLOBAL.lock().unwrap();
    let s = lock.as_ref().ok_or_else(|| "not initialised".to_owned())?;

    // Outbound wire frame: [u8 to_player_id][game data]
    let mut frame = Vec::with_capacity(1 + payload.len());
    frame.push(to_player_id);
    frame.extend_from_slice(payload);

    s.send_tx.send(frame).map_err(|_| "io task stopped".to_owned())
}

// ── I/O task ─────────────────────────────────────────────────────────────────
//
// Owns the socket and the bridge address. All actual network I/O runs here;
// the C thread only touches the channels.

async fn run_io(
    socket:      Arc<UdpSocket>,
    event_tx:    mpsc::UnboundedSender<WlEvent>,
    mut send_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    bridge_addr: SocketAddr,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let mut buf = vec![0u8; 65535];
    loop {
        tokio::select! {
            _ = &mut shutdown_rx => break,

            Some(frame) = send_rx.recv() => {
                let _ = socket.send_to(&frame, bridge_addr).await;
            }

            result = socket.recv_from(&mut buf) => {
                let Ok((n, _)) = result else { break };
                if n < 1 { continue; }

                // Inbound wire frame: [u8 from_player_id][game data]
                let from_player_id = buf[0];
                let copy_len = (n - 1).min(WL_PACKET_MAX);

                let mut ev = WlEvent {
                    kind:           1, // WL_EVENT_PACKET
                    from_player_id,
                    _pad:           [0; 3],
                    data_len:       copy_len as i32,
                    data:           [0; WL_PACKET_MAX],
                };
                ev.data[..copy_len].copy_from_slice(&buf[1..1 + copy_len]);

                let _ = event_tx.send(ev);
            }
        }
    }
}

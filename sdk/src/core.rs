// All SDK implementation logic. No unsafe, no C types.

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc as tmpsc, oneshot};

use crate::state::{CALLBACK, GLOBAL, RecvCallback, RecvCallbackFn, SdkState, WL_PACKET_MAX};

pub fn init(bridge_port: i32, local_player_id: u8) -> Result<(), String> {
    shutdown();

    let bridge_addr: SocketAddr = format!("127.0.0.1:{bridge_port}").parse().unwrap();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_io()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;

    let socket = Arc::new(
        rt.block_on(UdpSocket::bind("127.0.0.1:0"))
            .map_err(|e| format!("bind: {e}"))?,
    );

    let (outbound_tx, outbound_rx) = tmpsc::unbounded_channel::<Vec<u8>>();
    let (shutdown_recv_tx, shutdown_recv_rx) = oneshot::channel::<()>();
    let (shutdown_send_tx, shutdown_send_rx) = oneshot::channel::<()>();

    rt.spawn(recv_task(Arc::clone(&socket), shutdown_recv_rx));
    rt.spawn(send_task(socket, bridge_addr, outbound_rx, shutdown_send_rx));

    *GLOBAL.lock().unwrap() = Some(SdkState {
        rt,
        local_player_id,
        outbound_tx,
        shutdown_tx: Some((shutdown_recv_tx, shutdown_send_tx)),
    });

    Ok(())
}

/// Fires the registered callback on every inbound packet.
/// `data` pointer is into the stack buffer — valid only for the callback's duration.
async fn recv_task(socket: Arc<UdpSocket>, mut shutdown: oneshot::Receiver<()>) {
    let mut buf = [0u8; 1 + WL_PACKET_MAX];
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            res = socket.recv_from(&mut buf) => {
                let Ok((n, _)) = res else { break };
                // Inbound frame: [u8 from_player_id][data...]
                if n < 1 { continue; }
                let cb = CALLBACK.lock().unwrap().as_ref().copied();
                if let Some(cb) = cb {
                    unsafe {
                        (cb.func)(
                            buf[0],
                            buf[1..n].as_ptr() as *const std::ffi::c_void,
                            (n - 1) as std::ffi::c_int,
                            cb.userdata,
                        );
                    }
                }
            }
        }
    }
}

/// Drains the outbound channel → send_to.
async fn send_task(
    socket: Arc<UdpSocket>,
    bridge_addr: SocketAddr,
    mut rx: tmpsc::UnboundedReceiver<Vec<u8>>,
    mut shutdown: oneshot::Receiver<()>,
) {
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            frame = rx.recv() => {
                let Some(frame) = frame else { break };
                let _ = socket.send_to(&frame, bridge_addr).await;
            }
        }
    }
}

pub fn shutdown() {
    let taken = GLOBAL.lock().unwrap().take();
    if let Some(mut state) = taken {
        drop(state.shutdown_tx.take());
        drop(state.rt);
    }
}

pub fn local_player_id() -> u8 {
    GLOBAL.lock().unwrap().as_ref().map_or(0, |s| s.local_player_id)
}

pub fn set_callback(func: RecvCallbackFn, userdata: *mut std::ffi::c_void) {
    *CALLBACK.lock().unwrap() = Some(RecvCallback { func, userdata });
}

pub fn clear_callback() {
    *CALLBACK.lock().unwrap() = None;
}

/// Push one outbound frame. Non-blocking, never stalls the caller.
/// Outbound frame: [u8 to_player_id][data...]
pub fn send(to_player_id: u8, payload: &[u8]) -> Result<(), String> {
    let lock = GLOBAL.lock().unwrap();
    let s = lock.as_ref().ok_or_else(|| "not initialised".to_owned())?;

    let mut frame = Vec::with_capacity(1 + payload.len());
    frame.push(to_player_id);
    frame.extend_from_slice(payload);

    s.outbound_tx.send(frame).map_err(|_| "send channel closed".to_owned())
}

// C API boundary — argument conversion only; all logic is in core.

use std::ffi::{c_int, c_void};

use crate::core;
use crate::state::{ERROR, set_error};

#[unsafe(no_mangle)]
pub extern "C" fn wl_init(bridge_port: c_int, local_player_id: u8) -> c_int {
    match core::init(bridge_port, local_player_id) {
        Ok(()) => { set_error("OK"); 0 }
        Err(e) => { set_error(&e); -1 }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wl_shutdown() {
    core::shutdown();
}

#[unsafe(no_mangle)]
pub extern "C" fn wl_last_error() -> *const std::ffi::c_char {
    // SAFETY: ERROR is a static buffer valid for the process lifetime.
    // addr_of! avoids creating a shared reference to a static mut.
    std::ptr::addr_of!(ERROR) as *const std::ffi::c_char
}

#[unsafe(no_mangle)]
pub extern "C" fn wl_local_player_id() -> u8 {
    core::local_player_id()
}

/// Receive one pending inbound packet into `buf`.
/// Returns bytes written (> 0), 0 if no packet available, -1 on error.
/// Call in a loop each game tick until it returns 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wl_recv(
    from_player_id: *mut u8,
    buf: *mut c_void,
    buf_len: c_int,
) -> c_int {
    if from_player_id.is_null() || buf.is_null() || buf_len <= 0 {
        set_error("wl_recv: invalid arguments");
        return -1;
    }
    let buf_slice = unsafe { std::slice::from_raw_parts_mut(buf as *mut u8, buf_len as usize) };
    match core::recv(buf_slice) {
        Some((from, len)) => {
            unsafe { *from_player_id = from; }
            len as c_int
        }
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wl_send(
    to_player_id: u8,
    data: *const c_void,
    data_len: c_int,
) -> c_int {
    if data_len < 0 || (data_len > 0 && data.is_null()) {
        set_error("wl_send: invalid arguments");
        return -1;
    }
    let payload = if data_len > 0 {
        unsafe { std::slice::from_raw_parts(data as *const u8, data_len as usize) }
    } else {
        &[]
    };
    match core::send(to_player_id, payload) {
        Ok(()) => 0,
        Err(e) => { set_error(&e); -1 }
    }
}

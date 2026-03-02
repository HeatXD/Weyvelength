// C API boundary, argument conversion only; all logic is in core.

use std::ffi::{c_int, c_void};
use std::ptr;

use crate::core;
use crate::state::{WlEvent, error_lock, set_error};

#[unsafe(no_mangle)]
pub extern "C" fn wl_init(bridge_port: c_int, local_player_id: u8) -> c_int {
    match core::init(bridge_port, local_player_id) {
        Ok(()) => {
            set_error("OK");
            0
        }
        Err(e) => {
            set_error(&e);
            -1
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn wl_shutdown() {
    core::shutdown();
}

#[unsafe(no_mangle)]
pub extern "C" fn wl_last_error() -> *const std::ffi::c_char {
    error_lock().lock().unwrap().as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn wl_local_player_id() -> u8 {
    core::local_player_id()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wl_poll(out_count: *mut c_int) -> *const WlEvent {
    if out_count.is_null() {
        return ptr::null();
    }
    let (ptr, count) = core::poll();
    unsafe {
        *out_count = count;
    }
    ptr
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wl_send(to_player_id: u8, data: *const c_void, data_len: c_int) -> c_int {
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
        Err(e) => {
            set_error(format!("wl_send: {e}"));
            -1
        }
    }
}

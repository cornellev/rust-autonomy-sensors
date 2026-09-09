//! Raw bindings to shim/zed_shim.{h,cpp}. Keep this file a 1:1 mirror of the
//! header; put anything resembling policy or a nicer API in lib.rs instead.
use std::os::raw::{c_int, c_void};

#[allow(non_camel_case_types)]
pub type zed_shim_handle = *mut c_void;

unsafe extern "C" {
    pub fn zed_shim_create() -> zed_shim_handle;
    pub fn zed_shim_open(cam: zed_shim_handle, fps: c_int) -> c_int;
    pub fn zed_shim_grab(cam: zed_shim_handle) -> c_int;
    pub fn zed_shim_width(cam: zed_shim_handle) -> c_int;
    pub fn zed_shim_height(cam: zed_shim_handle) -> c_int;
    pub fn zed_shim_get_image_bgra(cam: zed_shim_handle, dst: *mut u8, dst_len: usize) -> c_int;
    pub fn zed_shim_timestamp_ns(cam: zed_shim_handle) -> u64;
    pub fn zed_shim_close(cam: zed_shim_handle);
    pub fn zed_shim_destroy(cam: zed_shim_handle);
}

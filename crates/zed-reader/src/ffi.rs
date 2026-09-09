//! Raw bindings to cpp/zed_camera.{h,cpp}. Keep this file a 1:1 mirror of
//! the header; put anything resembling policy or a nicer API in lib.rs
//! instead.
use std::os::raw::{c_int, c_void};

#[allow(non_camel_case_types)]
pub type zed_camera_handle = *mut c_void;

unsafe extern "C" {
    pub fn zed_camera_create() -> zed_camera_handle;
    pub fn zed_camera_open(cam: zed_camera_handle, fps: c_int, depth_mode: c_int) -> c_int;
    pub fn zed_camera_grab(cam: zed_camera_handle) -> c_int;
    pub fn zed_camera_width(cam: zed_camera_handle) -> c_int;
    pub fn zed_camera_height(cam: zed_camera_handle) -> c_int;
    pub fn zed_camera_get_image_bgra(cam: zed_camera_handle, dst: *mut u8, dst_len: usize)
    -> c_int;
    pub fn zed_camera_get_depth_f32(
        cam: zed_camera_handle,
        dst: *mut f32,
        dst_len_floats: usize,
    ) -> c_int;
    pub fn zed_camera_timestamp_ns(cam: zed_camera_handle) -> u64;
    pub fn zed_camera_close(cam: zed_camera_handle);
    pub fn zed_camera_destroy(cam: zed_camera_handle);
}

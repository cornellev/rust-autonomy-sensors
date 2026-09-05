//! Concrete ZED SHM interface. See crate README. Depend on `ZedFrame`,
//! `ZedDepthFrame`, and the `*_SERVICE_NAME` constants directly, not a
//! generic sensor abstraction.
pub mod ffi;

use anyhow::{Result, bail};
use iceoryx2::prelude::*;
use std::fmt::Debug;
use std::mem::MaybeUninit;

/// VGA. Higher resolutions failed over the dev-machine USB/IP passthrough.
/// Re-check on the Jetson (no USB/IP there). See README.
pub const WIDTH: usize = 672;
pub const HEIGHT: usize = 376;
pub const BGRA_LEN: usize = WIDTH * HEIGHT * 4;
pub const DEPTH_LEN: usize = WIDTH * HEIGHT;

pub const IMAGE_SERVICE_NAME: &str = "zed/zed2/image_left_vga_bgra";
pub const DEPTH_SERVICE_NAME: &str = "zed/zed2/depth_vga_f32";

/// One published image frame. `#[repr(C)]` and every field `ZeroCopySend`:
/// an `iceoryx2` payload type requirement.
#[repr(C)]
#[derive(Debug, ZeroCopySend)]
pub struct ZedFrame {
    /// Per-process counter. Resets on restart.
    pub frame_id: u64,
    /// ZED SDK clock, `sl::TIME_REFERENCE::IMAGE`, nanoseconds.
    pub timestamp_ns: u64,
    pub width: u32,
    pub height: u32,
    pub data: [u8; BGRA_LEN],
}

/// One published depth frame. Same field meanings as `ZedFrame`.
#[repr(C)]
#[derive(Debug, ZeroCopySend)]
pub struct ZedDepthFrame {
    pub frame_id: u64,
    pub timestamp_ns: u64,
    pub width: u32,
    pub height: u32,
    /// Meters. NaN or inf where the SDK has no valid depth for that pixel.
    pub data: [f32; DEPTH_LEN],
}

/// RAII wrapper around the zed_shim handle. Fixed at VGA, neural depth.
pub struct ZedCamera {
    handle: ffi::zed_shim_handle,
}

unsafe impl Send for ZedCamera {}

impl ZedCamera {
    pub fn open(fps: i32) -> Result<Self> {
        let handle = unsafe { ffi::zed_shim_create() };
        if handle.is_null() {
            bail!("zed_shim_create returned null");
        }
        let err = unsafe { ffi::zed_shim_open(handle, fps) };
        if err != 0 {
            unsafe { ffi::zed_shim_destroy(handle) };
            bail!("zed_shim_open failed: sl::ERROR_CODE = {err}");
        }
        Ok(Self { handle })
    }

    /// Grabs one frame and writes image and depth into `image_out` and
    /// `depth_out` in place: one SDK grab() serves both, and both args take
    /// `MaybeUninit` so this can write straight into loaned iceoryx2 samples
    /// (shared memory) with no full-frame stack copy.
    pub fn grab_into(
        &mut self,
        frame_id: u64,
        image_out: &mut MaybeUninit<ZedFrame>,
        depth_out: &mut MaybeUninit<ZedDepthFrame>,
    ) -> Result<()> {
        let err = unsafe { ffi::zed_shim_grab(self.handle) };
        if err != 0 {
            bail!("zed_shim_grab failed: sl::ERROR_CODE = {err}");
        }
        let width = unsafe { ffi::zed_shim_width(self.handle) };
        let height = unsafe { ffi::zed_shim_height(self.handle) };
        if width as usize != WIDTH || height as usize != HEIGHT {
            bail!("unexpected resolution {width}x{height}, expected {WIDTH}x{HEIGHT}");
        }
        let timestamp_ns = unsafe { ffi::zed_shim_timestamp_ns(self.handle) };

        let img_ptr = image_out.as_mut_ptr();
        let depth_ptr = depth_out.as_mut_ptr();
        unsafe {
            let img_data = std::ptr::addr_of_mut!((*img_ptr).data) as *mut u8;
            if ffi::zed_shim_get_image_bgra(self.handle, img_data, BGRA_LEN) != 0 {
                bail!("zed_shim_get_image_bgra failed");
            }
            std::ptr::addr_of_mut!((*img_ptr).frame_id).write(frame_id);
            std::ptr::addr_of_mut!((*img_ptr).timestamp_ns).write(timestamp_ns);
            std::ptr::addr_of_mut!((*img_ptr).width).write(width as u32);
            std::ptr::addr_of_mut!((*img_ptr).height).write(height as u32);

            let depth_data = std::ptr::addr_of_mut!((*depth_ptr).data) as *mut f32;
            if ffi::zed_shim_get_depth_f32(self.handle, depth_data, DEPTH_LEN) != 0 {
                bail!("zed_shim_get_depth_f32 failed");
            }
            std::ptr::addr_of_mut!((*depth_ptr).frame_id).write(frame_id);
            std::ptr::addr_of_mut!((*depth_ptr).timestamp_ns).write(timestamp_ns);
            std::ptr::addr_of_mut!((*depth_ptr).width).write(width as u32);
            std::ptr::addr_of_mut!((*depth_ptr).height).write(height as u32);
        }
        Ok(())
    }
}

impl Drop for ZedCamera {
    fn drop(&mut self) {
        unsafe {
            ffi::zed_shim_close(self.handle);
            ffi::zed_shim_destroy(self.handle);
        }
    }
}

/// Opens (or creates) a publish-subscribe service for payload type `T`. One
/// place for publisher and subscriber binaries to agree on name and type.
pub fn open_service<T: Debug + ZeroCopySend>(
    node: &Node<ipc::Service>,
    name: &str,
) -> Result<iceoryx2::service::port_factory::publish_subscribe::PortFactory<ipc::Service, T, ()>> {
    let service = node
        .service_builder(&name.try_into()?)
        .publish_subscribe::<T>()
        .open_or_create()?;
    Ok(service)
}

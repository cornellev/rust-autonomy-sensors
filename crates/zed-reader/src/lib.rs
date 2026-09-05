//! ZED camera capture + the shared-memory interface it publishes on.
//!
//! This is a concrete, single-sensor interface, not a standardized one --
//! see the repo README. Anyone consuming ZED frames should depend on
//! [`ZedFrame`] and [`IMAGE_SERVICE_NAME`] directly rather than expecting a
//! generic sensor abstraction.
pub mod ffi;

use anyhow::{Result, bail};
use iceoryx2::prelude::*;

/// Fixed for now: the SDK diagnostic showed higher resolutions are
/// unreliable over the WSL USB/IP passthrough used during development, and
/// VGA is what the pipeline this is replacing already used. Revisit once
/// this is validated on the Jetson directly (no USB/IP in the loop there).
pub const WIDTH: usize = 672;
pub const HEIGHT: usize = 376;
pub const BGRA_LEN: usize = WIDTH * HEIGHT * 4;

/// iceoryx2 service name for the left BGRA image stream. One service per
/// concern, matching one physical sensor -- see crate README.
pub const IMAGE_SERVICE_NAME: &str = "zed/zed2/image_left_vga_bgra";

/// One published frame. `#[repr(C)]` and every field `ZeroCopySend` per
/// iceoryx2's requirements for a zero-copy payload type.
#[repr(C)]
#[derive(Debug, ZeroCopySend)]
pub struct ZedFrame {
    /// Monotonically increasing per-process, not persisted across restarts.
    pub frame_id: u64,
    /// From the ZED SDK's own clock (`sl::TIME_REFERENCE::IMAGE`), nanoseconds.
    pub timestamp_ns: u64,
    pub width: u32,
    pub height: u32,
    pub data: [u8; BGRA_LEN],
}

/// Safe RAII wrapper around the zed_shim handle. Fixed at VGA/no-depth; see
/// `WIDTH`/`HEIGHT`.
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

    /// Blocks until a frame is available, then writes it directly into
    /// `out` in place via raw pointer field writes -- deliberately takes
    /// `MaybeUninit` rather than `&mut ZedFrame` so this can write straight
    /// into a loaned-but-uninitialized iceoryx2 sample (i.e. directly into
    /// shared memory) without ever materializing a ~1 MiB `ZedFrame` on the
    /// stack first, and without forming a `&mut ZedFrame` over memory that
    /// isn't fully initialized yet.
    pub fn grab_into(
        &mut self,
        frame_id: u64,
        out: &mut std::mem::MaybeUninit<ZedFrame>,
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

        let ptr = out.as_mut_ptr();
        unsafe {
            let data_ptr = std::ptr::addr_of_mut!((*ptr).data) as *mut u8;
            let copy_err = ffi::zed_shim_get_image_bgra(self.handle, data_ptr, BGRA_LEN);
            if copy_err != 0 {
                bail!("zed_shim_get_image_bgra failed");
            }
            let timestamp_ns = ffi::zed_shim_timestamp_ns(self.handle);
            std::ptr::addr_of_mut!((*ptr).frame_id).write(frame_id);
            std::ptr::addr_of_mut!((*ptr).timestamp_ns).write(timestamp_ns);
            std::ptr::addr_of_mut!((*ptr).width).write(width as u32);
            std::ptr::addr_of_mut!((*ptr).height).write(height as u32);
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

/// Opens (or creates) the image publish-subscribe service. Shared by the
/// reader (publisher side) and any consumer/debug tooling (subscriber side)
/// so both agree on the exact service name and payload type in one place.
pub fn open_image_service(
    node: &Node<ipc::Service>,
) -> Result<
    iceoryx2::service::port_factory::publish_subscribe::PortFactory<ipc::Service, ZedFrame, ()>,
> {
    let service = node
        .service_builder(&IMAGE_SERVICE_NAME.try_into()?)
        .publish_subscribe::<ZedFrame>()
        .open_or_create()?;
    Ok(service)
}

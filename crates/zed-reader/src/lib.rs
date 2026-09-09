//! Concrete ZED SHM interface. See crate README. Depend on `ZedFrame` and
//! `IMAGE_SERVICE_NAME` directly, not a generic sensor abstraction.
pub mod ffi;

use anyhow::{Result, bail};
use iceoryx2::prelude::*;

/// VGA. Higher resolutions failed over the dev-machine USB/IP passthrough.
/// Re-check on the Jetson (no USB/IP there). See README.
pub const WIDTH: usize = 672;
pub const HEIGHT: usize = 376;
pub const BGRA_LEN: usize = WIDTH * HEIGHT * 4;

pub const IMAGE_SERVICE_NAME: &str = "zed/zed2/image_left_vga_bgra";

/// One published frame. `#[repr(C)]` and every field `ZeroCopySend`: an
/// `iceoryx2` payload type requirement.
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

/// RAII wrapper around the zed_shim handle. Fixed at VGA, no depth.
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

    /// Grabs one frame and writes it into `out` in place. Takes
    /// `MaybeUninit`, not `&mut ZedFrame`: this can write straight into a
    /// loaned iceoryx2 sample (shared memory) with no full-frame stack copy,
    /// and never forms a reference to not-yet-initialized memory.
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
            if ffi::zed_shim_get_image_bgra(self.handle, data_ptr, BGRA_LEN) != 0 {
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

/// Opens (or creates) the image publish-subscribe service. One place for
/// publisher and subscriber binaries to agree on name and type.
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

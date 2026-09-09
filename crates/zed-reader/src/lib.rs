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

/// `sl::DEPTH_MODE` values this crate exposes. Skips `PERFORMANCE`/
/// `QUALITY`/`ULTRA`: deprecated in the SDK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepthMode {
    /// Disables depth. `ZedCamera::grab` then skips the depth pipeline
    /// entirely -- no GPU inference cost, not just no publish.
    None,
    NeuralLight,
    Neural,
    NeuralPlus,
}

impl DepthMode {
    fn sl_value(self) -> i32 {
        match self {
            DepthMode::None => 0,
            DepthMode::NeuralLight => 4,
            DepthMode::Neural => 5,
            DepthMode::NeuralPlus => 6,
        }
    }

    /// Parses `none` | `neural_light` | `neural` | `neural_plus`, case
    /// insensitive. Used for the `ZED_READER_DEPTH_MODE` env var.
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "none" => Ok(DepthMode::None),
            "neural_light" => Ok(DepthMode::NeuralLight),
            "neural" => Ok(DepthMode::Neural),
            "neural_plus" => Ok(DepthMode::NeuralPlus),
            other => {
                bail!("unknown depth mode `{other}` (want none|neural_light|neural|neural_plus)")
            }
        }
    }
}

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

/// RAII wrapper around the zed_camera handle. Fixed at VGA.
pub struct ZedCamera {
    handle: ffi::zed_camera_handle,
    depth_enabled: bool,
}

unsafe impl Send for ZedCamera {}

impl ZedCamera {
    pub fn open(fps: i32, depth_mode: DepthMode) -> Result<Self> {
        let handle = unsafe { ffi::zed_camera_create() };
        if handle.is_null() {
            bail!("zed_camera_create returned null");
        }
        let err = unsafe { ffi::zed_camera_open(handle, fps, depth_mode.sl_value()) };
        if err != 0 {
            unsafe { ffi::zed_camera_destroy(handle) };
            bail!("zed_camera_open failed: sl::ERROR_CODE = {err}");
        }
        Ok(Self {
            handle,
            depth_enabled: depth_mode != DepthMode::None,
        })
    }

    pub fn depth_enabled(&self) -> bool {
        self.depth_enabled
    }

    /// Grabs one frame. Follow with `read_image_into`, and `read_depth_into`
    /// if depth is enabled, to get it.
    pub fn grab(&mut self) -> Result<()> {
        let err = unsafe { ffi::zed_camera_grab(self.handle) };
        if err != 0 {
            bail!("zed_camera_grab failed: sl::ERROR_CODE = {err}");
        }
        Ok(())
    }

    fn checked_dims(&self) -> Result<(u32, u32)> {
        let width = unsafe { ffi::zed_camera_width(self.handle) };
        let height = unsafe { ffi::zed_camera_height(self.handle) };
        if width as usize != WIDTH || height as usize != HEIGHT {
            bail!("unexpected resolution {width}x{height}, expected {WIDTH}x{HEIGHT}");
        }
        Ok((width as u32, height as u32))
    }

    /// Writes the last `grab()`'d image into `out` in place. Takes
    /// `MaybeUninit`, not `&mut ZedFrame`: this can write straight into a
    /// loaned iceoryx2 sample (shared memory) with no full-frame stack copy.
    pub fn read_image_into(&self, frame_id: u64, out: &mut MaybeUninit<ZedFrame>) -> Result<()> {
        let (width, height) = self.checked_dims()?;
        let timestamp_ns = unsafe { ffi::zed_camera_timestamp_ns(self.handle) };
        let ptr = out.as_mut_ptr();
        unsafe {
            let data = std::ptr::addr_of_mut!((*ptr).data) as *mut u8;
            if ffi::zed_camera_get_image_bgra(self.handle, data, BGRA_LEN) != 0 {
                bail!("zed_camera_get_image_bgra failed");
            }
            std::ptr::addr_of_mut!((*ptr).frame_id).write(frame_id);
            std::ptr::addr_of_mut!((*ptr).timestamp_ns).write(timestamp_ns);
            std::ptr::addr_of_mut!((*ptr).width).write(width);
            std::ptr::addr_of_mut!((*ptr).height).write(height);
        }
        Ok(())
    }

    /// Writes the last `grab()`'d depth map into `out` in place. Errors if
    /// this camera was opened with `DepthMode::None`.
    pub fn read_depth_into(
        &self,
        frame_id: u64,
        out: &mut MaybeUninit<ZedDepthFrame>,
    ) -> Result<()> {
        if !self.depth_enabled {
            bail!("read_depth_into called but depth is disabled (opened with DepthMode::None)");
        }
        let (width, height) = self.checked_dims()?;
        let timestamp_ns = unsafe { ffi::zed_camera_timestamp_ns(self.handle) };
        let ptr = out.as_mut_ptr();
        unsafe {
            let data = std::ptr::addr_of_mut!((*ptr).data) as *mut f32;
            if ffi::zed_camera_get_depth_f32(self.handle, data, DEPTH_LEN) != 0 {
                bail!("zed_camera_get_depth_f32 failed");
            }
            std::ptr::addr_of_mut!((*ptr).frame_id).write(frame_id);
            std::ptr::addr_of_mut!((*ptr).timestamp_ns).write(timestamp_ns);
            std::ptr::addr_of_mut!((*ptr).width).write(width);
            std::ptr::addr_of_mut!((*ptr).height).write(height);
        }
        Ok(())
    }
}

impl Drop for ZedCamera {
    fn drop(&mut self) {
        unsafe {
            ffi::zed_camera_close(self.handle);
            ffi::zed_camera_destroy(self.handle);
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

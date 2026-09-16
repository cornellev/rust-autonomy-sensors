//! ZED camera access and the concrete Zenoh payload contract used by this
//! crate. Frames are written directly into Zenoh shared-memory buffers.
pub mod ffi;

use anyhow::{Context, Result, bail, ensure};
use zenoh::{
    Wait,
    shm::{
        AllocAlignment, GarbageCollect, MemoryLayout, PosixShmProviderBackend, ShmProvider,
        ShmProviderBuilder, ZShmMut,
    },
};

#[cfg(not(target_endian = "little"))]
compile_error!("the ZED frame wire format currently requires a little-endian target");

/// VGA. Higher resolutions failed over the dev-machine USB/IP passthrough.
/// Re-check on the Jetson (no USB/IP there). See README.
pub const WIDTH: usize = 672;
pub const HEIGHT: usize = 376;
pub const BGRA_LEN: usize = WIDTH * HEIGHT * 4;
pub const DEPTH_LEN: usize = WIDTH * HEIGHT;
pub const DEPTH_BYTES_LEN: usize = DEPTH_LEN * size_of::<f32>();

pub const IMAGE_KEY_EXPR: &str = "zed/zed2/image_left_vga_bgra";
pub const DEPTH_KEY_EXPR: &str = "zed/zed2/depth_vga_f32";
pub const FRAME_ENCODING: &str = "application/vnd.cornellev.zed-frame";

/// Both frame kinds use the same fixed-size, versioned wire header:
/// magic[4], version[1], kind[1], reserved[2], frame_id[8], timestamp_ns[8],
/// width[4], height[4]. Integer fields are little-endian.
pub const FRAME_HEADER_LEN: usize = 32;
pub const IMAGE_FRAME_LEN: usize = FRAME_HEADER_LEN + BGRA_LEN;
pub const DEPTH_FRAME_LEN: usize = FRAME_HEADER_LEN + DEPTH_BYTES_LEN;

/// Enough for roughly 32 VGA image/depth frame pairs. Held subscriber samples
/// keep their allocations alive, so deployments with more in-flight frames
/// should raise `ZED_READER_SHM_POOL_SIZE_MIB`.
pub const DEFAULT_SHM_POOL_SIZE_BYTES: usize = 64 * 1024 * 1024;

const FRAME_MAGIC: &[u8; 4] = b"ZED1";
const FRAME_VERSION: u8 = 1;

pub type ZedShmProvider = ShmProvider<PosixShmProviderBackend>;

/// Loads a Zenoh JSON/JSON5 configuration when `ZED_READER_ZENOH_CONFIG` is
/// set, otherwise uses Zenoh's default peer configuration.
pub fn zenoh_config_from_env() -> Result<zenoh::Config> {
    match std::env::var("ZED_READER_ZENOH_CONFIG") {
        Ok(path) => zenoh::Config::from_file(&path)
            .map_err(|error| anyhow::anyhow!("load Zenoh config from {path}: {error}")),
        Err(std::env::VarError::NotPresent) => Ok(zenoh::Config::default()),
        Err(error) => Err(error).context("read ZED_READER_ZENOH_CONFIG"),
    }
}

/// Parses the optional SHM pool size. The value is in MiB so operators do not
/// need to provide large byte counts in deployment manifests.
pub fn shm_pool_size_from_env() -> Result<usize> {
    let Some(mib) = std::env::var("ZED_READER_SHM_POOL_SIZE_MIB")
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .with_context(|| format!("invalid ZED_READER_SHM_POOL_SIZE_MIB `{value}`"))
        })
        .transpose()?
    else {
        return Ok(DEFAULT_SHM_POOL_SIZE_BYTES);
    };
    mib.checked_mul(1024 * 1024)
        .context("ZED_READER_SHM_POOL_SIZE_MIB overflows usize")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum FrameKind {
    ImageBgra = 1,
    DepthF32 = 2,
}

/// Metadata decoded from a frame payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    pub frame_id: u64,
    pub timestamp_ns: u64,
    pub width: u32,
    pub height: u32,
}

/// A borrowed image frame. `data` points into the received `ZBytes`; it is not
/// copied when the payload is contiguous (including Zenoh SHM delivery).
#[derive(Debug, Clone, Copy)]
pub struct ZedFrame<'a> {
    pub frame_id: u64,
    pub timestamp_ns: u64,
    pub width: u32,
    pub height: u32,
    pub data: &'a [u8],
}

/// A borrowed depth frame. The data is encoded as little-endian `f32` values.
#[derive(Debug, Clone, Copy)]
pub struct ZedDepthFrame<'a> {
    pub frame_id: u64,
    pub timestamp_ns: u64,
    pub width: u32,
    pub height: u32,
    data: &'a [u8],
}

impl ZedDepthFrame<'_> {
    pub fn values(&self) -> impl ExactSizeIterator<Item = f32> + '_ {
        self.data
            .as_chunks::<{ size_of::<f32>() }>()
            .0
            .iter()
            .map(|bytes| f32::from_le_bytes(*bytes))
    }

    pub fn value_at(&self, index: usize) -> Option<f32> {
        let start = index.checked_mul(size_of::<f32>())?;
        let bytes = self.data.get(start..start + size_of::<f32>())?;
        Some(f32::from_le_bytes(
            bytes.try_into().expect("four-byte slice"),
        ))
    }

    pub fn data_bytes(&self) -> &[u8] {
        self.data
    }
}

/// Creates the POSIX SHM pool used for explicit Zenoh SHM allocations.
pub fn create_shm_provider(capacity: usize) -> Result<ZedShmProvider> {
    ensure!(
        capacity >= IMAGE_FRAME_LEN.max(DEPTH_FRAME_LEN),
        "Zenoh SHM pool ({capacity} bytes) is smaller than one frame"
    );
    let layout = MemoryLayout::new(capacity, AllocAlignment::ALIGN_8_BYTES)
        .context("construct aligned Zenoh SHM pool layout")?;
    ShmProviderBuilder::default_backend(layout)
        .wait()
        .map_err(|error| anyhow::anyhow!("create Zenoh POSIX SHM provider: {error}"))
}

/// Allocates one aligned Zenoh SHM frame. Garbage collection is attempted on
/// exhaustion, but allocation does not block the camera loop behind a slow
/// subscriber.
pub fn allocate_frame(provider: &ZedShmProvider, len: usize) -> Result<ZShmMut> {
    let layout = MemoryLayout::new(len, AllocAlignment::ALIGN_8_BYTES)
        .context("construct aligned frame layout")?;
    provider
        .alloc(layout)
        .with_policy::<GarbageCollect>()
        .wait()
        .context("allocate Zenoh SHM frame")
}

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

    /// Writes the last `grab()`'d image directly into a complete Zenoh frame
    /// payload. Passing a `ZShmMut` slice avoids a full-frame staging copy.
    pub fn read_image_into(&self, frame_id: u64, out: &mut [u8]) -> Result<()> {
        ensure!(
            out.len() == IMAGE_FRAME_LEN,
            "image output is {} bytes, expected {IMAGE_FRAME_LEN}",
            out.len()
        );
        let (width, height) = self.checked_dims()?;
        let timestamp_ns = unsafe { ffi::zed_camera_timestamp_ns(self.handle) };
        let data = &mut out[FRAME_HEADER_LEN..];
        if unsafe { ffi::zed_camera_get_image_bgra(self.handle, data.as_mut_ptr(), data.len()) }
            != 0
        {
            bail!("zed_camera_get_image_bgra failed");
        }
        write_header(
            out,
            FrameKind::ImageBgra,
            FrameHeader {
                frame_id,
                timestamp_ns,
                width,
                height,
            },
        );
        Ok(())
    }

    /// Writes the last `grab()`'d depth map directly into a complete Zenoh
    /// frame payload. Errors if depth was disabled when opening the camera.
    pub fn read_depth_into(&self, frame_id: u64, out: &mut [u8]) -> Result<()> {
        if !self.depth_enabled {
            bail!("read_depth_into called but depth is disabled (opened with DepthMode::None)");
        }
        ensure!(
            out.len() == DEPTH_FRAME_LEN,
            "depth output is {} bytes, expected {DEPTH_FRAME_LEN}",
            out.len()
        );
        let (width, height) = self.checked_dims()?;
        let timestamp_ns = unsafe { ffi::zed_camera_timestamp_ns(self.handle) };
        let data = &mut out[FRAME_HEADER_LEN..];
        if unsafe { ffi::zed_camera_get_depth_f32(self.handle, data.as_mut_ptr(), data.len()) } != 0
        {
            bail!("zed_camera_get_depth_f32 failed");
        }
        write_header(
            out,
            FrameKind::DepthF32,
            FrameHeader {
                frame_id,
                timestamp_ns,
                width,
                height,
            },
        );
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

pub fn decode_image_frame(payload: &[u8]) -> Result<ZedFrame<'_>> {
    let (header, data) = decode_frame(payload, FrameKind::ImageBgra, IMAGE_FRAME_LEN)?;
    Ok(ZedFrame {
        frame_id: header.frame_id,
        timestamp_ns: header.timestamp_ns,
        width: header.width,
        height: header.height,
        data,
    })
}

pub fn decode_depth_frame(payload: &[u8]) -> Result<ZedDepthFrame<'_>> {
    let (header, data) = decode_frame(payload, FrameKind::DepthF32, DEPTH_FRAME_LEN)?;
    Ok(ZedDepthFrame {
        frame_id: header.frame_id,
        timestamp_ns: header.timestamp_ns,
        width: header.width,
        height: header.height,
        data,
    })
}

fn write_header(out: &mut [u8], kind: FrameKind, header: FrameHeader) {
    out[0..4].copy_from_slice(FRAME_MAGIC);
    out[4] = FRAME_VERSION;
    out[5] = kind as u8;
    out[6..8].fill(0);
    out[8..16].copy_from_slice(&header.frame_id.to_le_bytes());
    out[16..24].copy_from_slice(&header.timestamp_ns.to_le_bytes());
    out[24..28].copy_from_slice(&header.width.to_le_bytes());
    out[28..32].copy_from_slice(&header.height.to_le_bytes());
}

fn decode_frame(
    payload: &[u8],
    kind: FrameKind,
    expected_len: usize,
) -> Result<(FrameHeader, &[u8])> {
    ensure!(
        payload.len() == expected_len,
        "frame is {} bytes, expected {expected_len}",
        payload.len()
    );
    ensure!(&payload[0..4] == FRAME_MAGIC, "invalid ZED frame magic");
    ensure!(
        payload[4] == FRAME_VERSION,
        "unsupported ZED frame version {}",
        payload[4]
    );
    ensure!(
        payload[5] == kind as u8,
        "unexpected ZED frame kind {}",
        payload[5]
    );
    ensure!(payload[6..8] == [0, 0], "non-zero reserved header bytes");

    let header = FrameHeader {
        frame_id: u64::from_le_bytes(payload[8..16].try_into().expect("checked header length")),
        timestamp_ns: u64::from_le_bytes(
            payload[16..24].try_into().expect("checked header length"),
        ),
        width: u32::from_le_bytes(payload[24..28].try_into().expect("checked header length")),
        height: u32::from_le_bytes(payload[28..32].try_into().expect("checked header length")),
    };
    ensure!(
        header.width as usize == WIDTH && header.height as usize == HEIGHT,
        "unexpected frame dimensions {}x{}, expected {WIDTH}x{HEIGHT}",
        header.width,
        header.height
    );
    Ok((header, &payload[FRAME_HEADER_LEN..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zenoh::{bytes::ZBytes, handlers::RingChannel};

    #[test]
    fn image_wire_format_round_trips() {
        let mut payload = vec![0; IMAGE_FRAME_LEN];
        let header = FrameHeader {
            frame_id: 42,
            timestamp_ns: 123_456_789,
            width: WIDTH as u32,
            height: HEIGHT as u32,
        };
        write_header(&mut payload, FrameKind::ImageBgra, header);
        payload[FRAME_HEADER_LEN] = 17;

        let frame = decode_image_frame(&payload).unwrap();
        assert_eq!(frame.frame_id, header.frame_id);
        assert_eq!(frame.timestamp_ns, header.timestamp_ns);
        assert_eq!(frame.width, header.width);
        assert_eq!(frame.height, header.height);
        assert_eq!(frame.data.len(), BGRA_LEN);
        assert_eq!(frame.data[0], 17);
    }

    #[test]
    fn depth_values_decode_little_endian_floats() {
        let mut payload = vec![0; DEPTH_FRAME_LEN];
        write_header(
            &mut payload,
            FrameKind::DepthF32,
            FrameHeader {
                frame_id: 1,
                timestamp_ns: 2,
                width: WIDTH as u32,
                height: HEIGHT as u32,
            },
        );
        payload[FRAME_HEADER_LEN..FRAME_HEADER_LEN + 4].copy_from_slice(&1.25_f32.to_le_bytes());

        let frame = decode_depth_frame(&payload).unwrap();
        assert_eq!(frame.values().len(), DEPTH_LEN);
        assert_eq!(frame.value_at(0), Some(1.25));
        assert_eq!(frame.value_at(DEPTH_LEN), None);
    }

    #[test]
    fn decoder_rejects_wrong_kind_and_dimensions() {
        let mut payload = vec![0; IMAGE_FRAME_LEN];
        write_header(
            &mut payload,
            FrameKind::DepthF32,
            FrameHeader {
                frame_id: 0,
                timestamp_ns: 0,
                width: WIDTH as u32,
                height: HEIGHT as u32,
            },
        );
        assert!(decode_image_frame(&payload).is_err());

        write_header(
            &mut payload,
            FrameKind::ImageBgra,
            FrameHeader {
                frame_id: 0,
                timestamp_ns: 0,
                width: 1,
                height: HEIGHT as u32,
            },
        );
        assert!(decode_image_frame(&payload).is_err());
    }

    #[test]
    fn explicit_shm_payload_round_trips_through_zenoh() {
        let mut config = zenoh::Config::default();
        // This is an in-process data-path test; do not open network sockets.
        config.listen.endpoints.set(vec![]).unwrap();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session = zenoh::open(config).wait().unwrap();
        let key = format!("zed-reader/test/{}/image", std::process::id());
        let subscriber = session
            .declare_subscriber(&key)
            .with(RingChannel::new(1))
            .wait()
            .unwrap();
        let publisher = session.declare_publisher(&key).wait().unwrap();
        let provider = create_shm_provider(IMAGE_FRAME_LEN * 4).unwrap();
        let mut payload = allocate_frame(&provider, IMAGE_FRAME_LEN).unwrap();
        write_header(
            &mut payload,
            FrameKind::ImageBgra,
            FrameHeader {
                frame_id: 7,
                timestamp_ns: 8,
                width: WIDTH as u32,
                height: HEIGHT as u32,
            },
        );
        payload[FRAME_HEADER_LEN] = 99;

        publisher.put(payload).wait().unwrap();
        let sample = subscriber
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap()
            .expect("local Zenoh sample");
        assert!(sample.payload().as_shm().is_some());
        let bytes = sample.payload().to_bytes();
        let frame = decode_image_frame(&bytes).unwrap();
        assert_eq!(frame.frame_id, 7);
        assert_eq!(frame.data[0], 99);

        // Also exercise the public conversion expected by publishers.
        let second = allocate_frame(&provider, IMAGE_FRAME_LEN).unwrap();
        let bytes = ZBytes::from(second);
        assert!(bytes.as_shm().is_some());

        drop(sample);
        drop(publisher);
        drop(subscriber);
        session.close().wait().unwrap();
    }
}

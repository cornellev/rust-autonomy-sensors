//! ZED camera access and Zenoh shared-memory framing.
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
compile_error!("ZED frames require a little-endian target");

pub const WIDTH: usize = 672;
pub const HEIGHT: usize = 376;
pub const BGRA_LEN: usize = WIDTH * HEIGHT * 4;
pub const DEPTH_LEN: usize = WIDTH * HEIGHT;
pub const FRAME_HEADER_LEN: usize = 32;
pub const IMAGE_FRAME_LEN: usize = FRAME_HEADER_LEN + BGRA_LEN;
pub const DEPTH_FRAME_LEN: usize = FRAME_HEADER_LEN + DEPTH_LEN * size_of::<f32>();
pub const IMAGE_KEY_EXPR: &str = "zed/zed2/image_left_vga_bgra";
pub const DEPTH_KEY_EXPR: &str = "zed/zed2/depth_vga_f32";
pub const FRAME_ENCODING: &str = "application/vnd.cornellev.zed-frame";

const MAGIC: &[u8; 4] = b"ZED1";
const VERSION: u8 = 1;
const IMAGE_KIND: u8 = 1;
const DEPTH_KIND: u8 = 2;
const MIB: usize = 1024 * 1024;

pub type ZedShmProvider = ShmProvider<PosixShmProviderBackend>;

pub fn zenoh_config_from_env() -> Result<zenoh::Config> {
    match std::env::var("ZED_READER_ZENOH_CONFIG") {
        Ok(path) => zenoh::Config::from_file(&path)
            .map_err(|error| anyhow::anyhow!("load Zenoh config from {path}: {error}")),
        Err(std::env::VarError::NotPresent) => Ok(zenoh::Config::default()),
        Err(error) => Err(error.into()),
    }
}

pub fn shm_pool_size_from_env() -> Result<usize> {
    let mib = std::env::var("ZED_READER_SHM_POOL_SIZE_MIB").map_or(Ok(64), |value| {
        value
            .parse::<usize>()
            .context("invalid ZED_READER_SHM_POOL_SIZE_MIB")
    })?;
    mib.checked_mul(MIB).context("SHM pool size overflow")
}

pub struct ZedFrame<'a> {
    pub frame_id: u64,
    pub timestamp_ns: u64,
    pub width: u32,
    pub height: u32,
    pub data: &'a [u8],
}

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
            .as_chunks::<4>()
            .0
            .iter()
            .copied()
            .map(f32::from_le_bytes)
    }
}

pub fn create_shm_provider(capacity: usize) -> Result<ZedShmProvider> {
    ensure!(
        capacity >= IMAGE_FRAME_LEN.max(DEPTH_FRAME_LEN),
        "SHM pool is smaller than one frame"
    );
    let layout = MemoryLayout::new(capacity, AllocAlignment::ALIGN_8_BYTES)?;
    ShmProviderBuilder::default_backend(layout)
        .wait()
        .map_err(|error| anyhow::anyhow!("create Zenoh SHM provider: {error}"))
}

pub fn allocate_frame(provider: &ZedShmProvider, len: usize) -> Result<ZShmMut> {
    let layout = MemoryLayout::new(len, AllocAlignment::ALIGN_8_BYTES)?;
    provider
        .alloc(layout)
        .with_policy::<GarbageCollect>()
        .wait()
        .context("allocate Zenoh SHM frame")
}

#[derive(Debug, PartialEq, Eq)]
pub enum DepthMode {
    None,
    NeuralLight,
    Neural,
    NeuralPlus,
}

impl DepthMode {
    fn sl_value(self) -> i32 {
        match self {
            Self::None => 0,
            Self::NeuralLight => 4,
            Self::Neural => 5,
            Self::NeuralPlus => 6,
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "none" => Ok(Self::None),
            "neural_light" => Ok(Self::NeuralLight),
            "neural" => Ok(Self::Neural),
            "neural_plus" => Ok(Self::NeuralPlus),
            _ => bail!("invalid depth mode `{value}`"),
        }
    }
}

pub struct ZedCamera {
    handle: ffi::zed_camera_handle,
    depth_enabled: bool,
}

unsafe impl Send for ZedCamera {}

type Reader = unsafe extern "C" fn(ffi::zed_camera_handle, *mut u8, usize) -> i32;

impl ZedCamera {
    pub fn open(fps: i32, depth_mode: DepthMode) -> Result<Self> {
        let handle = unsafe { ffi::zed_camera_create() };
        if handle.is_null() {
            bail!("zed_camera_create returned null");
        }
        let depth_enabled = depth_mode != DepthMode::None;
        let error = unsafe { ffi::zed_camera_open(handle, fps, depth_mode.sl_value()) };
        if error != 0 {
            unsafe { ffi::zed_camera_destroy(handle) };
            bail!("zed_camera_open failed: {error}");
        }
        Ok(Self {
            handle,
            depth_enabled,
        })
    }

    pub fn depth_enabled(&self) -> bool {
        self.depth_enabled
    }

    pub fn grab(&mut self) -> Result<()> {
        match unsafe { ffi::zed_camera_grab(self.handle) } {
            0 => Ok(()),
            error => bail!("zed_camera_grab failed: {error}"),
        }
    }

    fn read_into(&self, frame_id: u64, out: &mut [u8], kind: u8, read: Reader) -> Result<()> {
        let expected = match kind {
            IMAGE_KIND => IMAGE_FRAME_LEN,
            _ => DEPTH_FRAME_LEN,
        };
        ensure!(out.len() == expected, "frame buffer has wrong length");
        let width = unsafe { ffi::zed_camera_width(self.handle) } as u32;
        let height = unsafe { ffi::zed_camera_height(self.handle) } as u32;
        ensure!(
            (width as usize, height as usize) == (WIDTH, HEIGHT),
            "unexpected resolution {width}x{height}"
        );
        let timestamp = unsafe { ffi::zed_camera_timestamp_ns(self.handle) };
        let data = &mut out[FRAME_HEADER_LEN..];
        ensure!(
            unsafe { read(self.handle, data.as_mut_ptr(), data.len()) } == 0,
            "read frame from ZED failed"
        );
        write_header(out, kind, (frame_id, timestamp, width, height));
        Ok(())
    }

    pub fn read_image_into(&self, frame_id: u64, out: &mut [u8]) -> Result<()> {
        self.read_into(frame_id, out, IMAGE_KIND, ffi::zed_camera_get_image_bgra)
    }

    pub fn read_depth_into(&self, frame_id: u64, out: &mut [u8]) -> Result<()> {
        ensure!(self.depth_enabled, "depth is disabled");
        self.read_into(frame_id, out, DEPTH_KIND, ffi::zed_camera_get_depth_f32)
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

type Header = (u64, u64, u32, u32);

fn write_header(out: &mut [u8], kind: u8, header: Header) {
    out[..4].copy_from_slice(MAGIC);
    out[4..8].copy_from_slice(&[VERSION, kind, 0, 0]);
    out[8..16].copy_from_slice(&header.0.to_le_bytes());
    out[16..24].copy_from_slice(&header.1.to_le_bytes());
    out[24..28].copy_from_slice(&header.2.to_le_bytes());
    out[28..32].copy_from_slice(&header.3.to_le_bytes());
}

fn field<const N: usize>(payload: &[u8], offset: usize) -> [u8; N] {
    payload[offset..offset + N]
        .try_into()
        .expect("validated frame length")
}

fn decode_frame(payload: &[u8], kind: u8, len: usize) -> Result<(Header, &[u8])> {
    ensure!(payload.len() == len, "frame has wrong length");
    ensure!(&payload[..4] == MAGIC, "invalid frame magic");
    ensure!(payload[4] == VERSION, "unsupported frame version");
    ensure!(
        payload[5] == kind && payload[6..8] == [0, 0],
        "invalid frame header"
    );
    let header = (
        u64::from_le_bytes(field(payload, 8)),
        u64::from_le_bytes(field(payload, 16)),
        u32::from_le_bytes(field(payload, 24)),
        u32::from_le_bytes(field(payload, 28)),
    );
    ensure!(
        (header.2 as usize, header.3 as usize) == (WIDTH, HEIGHT),
        "unexpected frame dimensions"
    );
    Ok((header, &payload[FRAME_HEADER_LEN..]))
}

pub fn decode_image_frame(payload: &[u8]) -> Result<ZedFrame<'_>> {
    let ((frame_id, timestamp_ns, width, height), data) =
        decode_frame(payload, IMAGE_KIND, IMAGE_FRAME_LEN)?;
    Ok(ZedFrame {
        frame_id,
        timestamp_ns,
        width,
        height,
        data,
    })
}

pub fn decode_depth_frame(payload: &[u8]) -> Result<ZedDepthFrame<'_>> {
    let ((frame_id, timestamp_ns, width, height), data) =
        decode_frame(payload, DEPTH_KIND, DEPTH_FRAME_LEN)?;
    Ok(ZedDepthFrame {
        frame_id,
        timestamp_ns,
        width,
        height,
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zenoh::handlers::RingChannel;

    fn frame(kind: u8, len: usize) -> Vec<u8> {
        let mut payload = vec![0; len];
        write_header(&mut payload, kind, (42, 123, WIDTH as u32, HEIGHT as u32));
        payload
    }

    #[test]
    fn codec() {
        let mut image = frame(IMAGE_KIND, IMAGE_FRAME_LEN);
        image[FRAME_HEADER_LEN] = 17;
        let decoded = decode_image_frame(&image).unwrap();
        assert_eq!(
            (decoded.frame_id, decoded.timestamp_ns, decoded.data[0]),
            (42, 123, 17)
        );

        let mut depth = frame(DEPTH_KIND, DEPTH_FRAME_LEN);
        depth[FRAME_HEADER_LEN..FRAME_HEADER_LEN + 4].copy_from_slice(&1.25_f32.to_le_bytes());
        assert_eq!(
            decode_depth_frame(&depth).unwrap().values().next(),
            Some(1.25)
        );
        assert!(decode_image_frame(&depth).is_err());
    }

    #[test]
    fn zenoh_preserves_shm_payload() {
        let mut config = zenoh::Config::default();
        config.listen.endpoints.set(vec![]).unwrap();
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
        let session = zenoh::open(config).wait().unwrap();
        let subscriber = session
            .declare_subscriber("zed-reader/test")
            .with(RingChannel::new(1))
            .wait()
            .unwrap();
        let publisher = session.declare_publisher("zed-reader/test").wait().unwrap();
        let provider = create_shm_provider(IMAGE_FRAME_LEN * 2).unwrap();
        let mut payload = allocate_frame(&provider, IMAGE_FRAME_LEN).unwrap();
        write_header(
            &mut payload,
            IMAGE_KIND,
            (7, 8, WIDTH as u32, HEIGHT as u32),
        );
        publisher.put(payload).wait().unwrap();
        let sample = subscriber
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap()
            .unwrap();
        assert!(sample.payload().as_shm().is_some());
        assert_eq!(
            decode_image_frame(&sample.payload().to_bytes())
                .unwrap()
                .frame_id,
            7
        );
    }
}

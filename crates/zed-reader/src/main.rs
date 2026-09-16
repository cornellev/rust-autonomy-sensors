use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use zed_reader::{
    DEPTH_FRAME_LEN, DEPTH_KEY_EXPR, DepthMode, FRAME_ENCODING, IMAGE_FRAME_LEN, IMAGE_KEY_EXPR,
    ZedCamera, allocate_frame, create_shm_provider, shm_pool_size_from_env, zenoh_config_from_env,
};
use zenoh::{Wait, pubsub::Publisher, qos::CongestionControl};

const FPS: i32 = 30;
const LOG_EVERY_N_FRAMES: u64 = 30;
static RUNNING: AtomicBool = AtomicBool::new(true);

fn publisher<'a>(session: &'a zenoh::Session, key: &'static str) -> Result<Publisher<'a>> {
    session
        .declare_publisher(key)
        .encoding(FRAME_ENCODING)
        .congestion_control(CongestionControl::Drop)
        .wait()
        .map_err(|error| anyhow::anyhow!("declare {key} publisher: {error}"))
}

fn publish(
    publisher: &Publisher<'_>,
    provider: &zed_reader::ZedShmProvider,
    len: usize,
    fill: impl FnOnce(&mut [u8]) -> Result<()>,
) -> Result<()> {
    let mut frame = allocate_frame(provider, len)?;
    fill(&mut frame)?;
    publisher
        .put(frame)
        .wait()
        .map_err(|error| anyhow::anyhow!("publish frame: {error}"))
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    ctrlc::set_handler(|| RUNNING.store(false, Ordering::SeqCst))?;

    let depth_mode = match std::env::var("ZED_READER_DEPTH_MODE") {
        Ok(s) => DepthMode::parse(&s)?,
        Err(_) => DepthMode::Neural,
    };
    let shm_pool_size = shm_pool_size_from_env()?;

    let session = zenoh::open(zenoh_config_from_env()?)
        .wait()
        .map_err(|error| anyhow::anyhow!("open Zenoh session: {error}"))?;
    let provider = create_shm_provider(shm_pool_size)?;
    let image_pub = publisher(&session, IMAGE_KEY_EXPR)?;

    let shm_pool_mib = shm_pool_size / 1024 / 1024;
    tracing::info!(?depth_mode, shm_pool_mib, "opening ZED camera");
    let mut camera = ZedCamera::open(FPS, depth_mode)?;
    let depth_pub = if camera.depth_enabled() {
        Some(publisher(&session, DEPTH_KEY_EXPR)?)
    } else {
        None
    };
    let mut frame_id = 0u64;
    let mut last_log_ts = std::time::Instant::now();

    while RUNNING.load(Ordering::SeqCst) {
        if let Err(error) = camera.grab() {
            tracing::warn!(%error, "grab failed, retrying");
            continue;
        }

        if let Err(error) = publish(&image_pub, &provider, IMAGE_FRAME_LEN, |frame| {
            camera.read_image_into(frame_id, frame)
        }) {
            tracing::warn!(%error, "image failed");
        }

        if let Some(depth_pub) = &depth_pub
            && let Err(error) = publish(depth_pub, &provider, DEPTH_FRAME_LEN, |frame| {
                camera.read_depth_into(frame_id, frame)
            })
        {
            tracing::warn!(%error, "depth failed");
        }

        frame_id += 1;
        if frame_id.is_multiple_of(LOG_EVERY_N_FRAMES) {
            let fps = LOG_EVERY_N_FRAMES as f64 / last_log_ts.elapsed().as_secs_f64();
            tracing::info!(frame_id, fps = format!("{fps:.1}"), "publishing");
            last_log_ts = std::time::Instant::now();
        }
    }

    tracing::info!("shutting down");
    Ok(())
}

//! Owns the ZED camera and publishes image + depth as explicit Zenoh shared-
//! memory payloads. See the crate README for the wire format and configuration.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use zed_reader::{
    DEPTH_FRAME_LEN, DEPTH_KEY_EXPR, DepthMode, FRAME_ENCODING, IMAGE_FRAME_LEN, IMAGE_KEY_EXPR,
    ZedCamera, allocate_frame, create_shm_provider, shm_pool_size_from_env, zenoh_config_from_env,
};
use zenoh::{Wait, qos::CongestionControl};

const FPS: i32 = 30;
const LOG_EVERY_N_FRAMES: u64 = 30;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let running = Arc::new(AtomicBool::new(true));
    let flag = Arc::clone(&running);
    ctrlc::set_handler(move || flag.store(false, Ordering::SeqCst))?;

    let depth_mode = match std::env::var("ZED_READER_DEPTH_MODE") {
        Ok(s) => DepthMode::parse(&s)?,
        Err(_) => DepthMode::Neural,
    };
    let shm_pool_size = shm_pool_size_from_env()?;

    let session = zenoh::open(zenoh_config_from_env()?)
        .wait()
        .map_err(|error| anyhow::anyhow!("open Zenoh session: {error}"))?;
    let provider = create_shm_provider(shm_pool_size)?;
    let image_pub = session
        .declare_publisher(IMAGE_KEY_EXPR)
        .encoding(FRAME_ENCODING)
        .congestion_control(CongestionControl::Drop)
        .wait()
        .map_err(|error| anyhow::anyhow!("declare image publisher: {error}"))?;

    tracing::info!(?depth_mode, "opening ZED camera at {FPS} fps (VGA)");
    let mut camera = ZedCamera::open(FPS, depth_mode)?;
    let depth_pub = if camera.depth_enabled() {
        Some(
            session
                .declare_publisher(DEPTH_KEY_EXPR)
                .encoding(FRAME_ENCODING)
                .congestion_control(CongestionControl::Drop)
                .wait()
                .map_err(|error| anyhow::anyhow!("declare depth publisher: {error}"))?,
        )
    } else {
        None
    };
    tracing::info!(
        image = IMAGE_KEY_EXPR,
        depth = DEPTH_KEY_EXPR,
        depth_enabled = camera.depth_enabled(),
        shm_pool_mib = shm_pool_size / (1024 * 1024),
        "publishing over Zenoh SHM"
    );

    let mut frame_id: u64 = 0;
    let mut last_log_ts = std::time::Instant::now();

    while running.load(Ordering::SeqCst) {
        if let Err(error) = camera.grab() {
            tracing::warn!(%error, "grab failed, retrying");
            continue;
        }

        let mut image = match allocate_frame(&provider, IMAGE_FRAME_LEN) {
            Ok(buffer) => buffer,
            Err(error) => {
                tracing::warn!(%error, "image SHM allocation failed");
                continue;
            }
        };
        if let Err(error) = camera.read_image_into(frame_id, &mut image) {
            tracing::warn!(%error, "read_image_into failed");
            continue;
        }
        if let Err(error) = image_pub.put(image).wait() {
            tracing::warn!(%error, "image publish failed");
        }

        if let Some(depth_pub) = &depth_pub {
            match allocate_frame(&provider, DEPTH_FRAME_LEN) {
                Ok(mut depth) => match camera.read_depth_into(frame_id, &mut depth) {
                    Ok(()) => {
                        if let Err(error) = depth_pub.put(depth).wait() {
                            tracing::warn!(%error, "depth publish failed");
                        }
                    }
                    Err(error) => tracing::warn!(%error, "read_depth_into failed"),
                },
                Err(error) => tracing::warn!(%error, "depth SHM allocation failed"),
            }
        }

        frame_id += 1;
        if frame_id.is_multiple_of(LOG_EVERY_N_FRAMES) {
            let elapsed = last_log_ts.elapsed();
            let fps = LOG_EVERY_N_FRAMES as f64 / elapsed.as_secs_f64();
            tracing::info!(frame_id, fps = format!("{fps:.1}"), "publishing");
            last_log_ts = std::time::Instant::now();
        }
    }

    tracing::info!("shutting down");
    drop(depth_pub);
    drop(image_pub);
    session
        .close()
        .wait()
        .map_err(|error| anyhow::anyhow!("close Zenoh session: {error}"))?;
    Ok(())
}

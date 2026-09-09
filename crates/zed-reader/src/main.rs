//! Owns the ZED camera and publishes image + depth over `iceoryx2`. See
//! crate README for the SHM interface and the `ZED_READER_DEPTH_MODE`
//! env var.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use iceoryx2::prelude::*;
use zed_reader::{
    DEPTH_SERVICE_NAME, DepthMode, IMAGE_SERVICE_NAME, ZedCamera, ZedDepthFrame, ZedFrame,
    open_service,
};

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
    tracing::info!(?depth_mode, "opening ZED camera at {FPS} fps (VGA)");
    let mut camera = ZedCamera::open(FPS, depth_mode)?;

    let node = NodeBuilder::new().create::<ipc::Service>()?;
    let image_pub = open_service::<ZedFrame>(&node, IMAGE_SERVICE_NAME)?
        .publisher_builder()
        .create()?;
    let depth_pub = if camera.depth_enabled() {
        Some(
            open_service::<ZedDepthFrame>(&node, DEPTH_SERVICE_NAME)?
                .publisher_builder()
                .create()?,
        )
    } else {
        None
    };
    tracing::info!(
        image = IMAGE_SERVICE_NAME,
        depth_enabled = camera.depth_enabled(),
        "publishing"
    );

    let mut frame_id: u64 = 0;
    let mut last_log_ts = std::time::Instant::now();

    while running.load(Ordering::SeqCst) {
        if let Err(e) = camera.grab() {
            tracing::warn!(error = %e, "grab failed, retrying");
            continue;
        }

        let mut image_sample = match image_pub.loan_uninit() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "image loan_uninit failed");
                continue;
            }
        };
        if let Err(e) = camera.read_image_into(frame_id, image_sample.payload_mut()) {
            tracing::warn!(error = %e, "read_image_into failed");
            continue;
        }
        // SAFETY: read_image_into wrote every field above, or returned Err.
        let image_sample = unsafe { image_sample.assume_init() };
        if let Err(e) = image_sample.send() {
            tracing::warn!(error = %e, "image send failed");
        }

        if let Some(depth_pub) = &depth_pub {
            match depth_pub.loan_uninit() {
                Ok(mut depth_sample) => {
                    match camera.read_depth_into(frame_id, depth_sample.payload_mut()) {
                        Ok(()) => {
                            // SAFETY: read_depth_into just wrote every field.
                            let depth_sample = unsafe { depth_sample.assume_init() };
                            if let Err(e) = depth_sample.send() {
                                tracing::warn!(error = %e, "depth send failed");
                            }
                        }
                        Err(e) => tracing::warn!(error = %e, "read_depth_into failed"),
                    }
                }
                Err(e) => tracing::warn!(error = %e, "depth loan_uninit failed"),
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
    Ok(())
}

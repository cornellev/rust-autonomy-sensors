//! Owns the ZED camera and publishes image + depth over `iceoryx2`. See
//! crate README for the SHM interface.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use iceoryx2::prelude::*;
use zed_reader::{
    DEPTH_SERVICE_NAME, IMAGE_SERVICE_NAME, ZedCamera, ZedDepthFrame, ZedFrame, open_service,
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

    tracing::info!("opening ZED camera at {FPS} fps (VGA, neural depth)");
    let mut camera = ZedCamera::open(FPS)?;

    let node = NodeBuilder::new().create::<ipc::Service>()?;
    let image_pub = open_service::<ZedFrame>(&node, IMAGE_SERVICE_NAME)?
        .publisher_builder()
        .create()?;
    let depth_pub = open_service::<ZedDepthFrame>(&node, DEPTH_SERVICE_NAME)?
        .publisher_builder()
        .create()?;
    tracing::info!(
        image = IMAGE_SERVICE_NAME,
        depth = DEPTH_SERVICE_NAME,
        "publishing"
    );

    let mut frame_id: u64 = 0;
    let mut last_log_ts = std::time::Instant::now();

    while running.load(Ordering::SeqCst) {
        let (mut image_sample, mut depth_sample) =
            match (image_pub.loan_uninit(), depth_pub.loan_uninit()) {
                (Ok(i), Ok(d)) => (i, d),
                _ => {
                    tracing::warn!("loan_uninit failed, retrying");
                    continue;
                }
            };

        if let Err(e) = camera.grab_into(
            frame_id,
            image_sample.payload_mut(),
            depth_sample.payload_mut(),
        ) {
            tracing::warn!(error = %e, "grab failed, retrying");
            continue;
        }

        // SAFETY: grab_into wrote every field of both payloads above, or
        // returned Err.
        let image_sample = unsafe { image_sample.assume_init() };
        let depth_sample = unsafe { depth_sample.assume_init() };
        if let Err(e) = image_sample.send() {
            tracing::warn!(error = %e, "image send failed");
        }
        if let Err(e) = depth_sample.send() {
            tracing::warn!(error = %e, "depth send failed");
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

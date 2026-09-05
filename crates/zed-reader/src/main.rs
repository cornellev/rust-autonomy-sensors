//! zed-reader: owns the physical ZED camera exclusively and publishes the
//! left image over shared memory (iceoryx2). See crate README for the SHM
//! interface (service name + payload layout) and lib.rs for both.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use iceoryx2::prelude::*;
use zed_reader::{ZedCamera, open_image_service};

const FPS: i32 = 30;
const LOG_EVERY_N_FRAMES: u64 = 30;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let running = Arc::new(AtomicBool::new(true));
    let flag = Arc::clone(&running);
    ctrlc::set_handler(move || flag.store(false, Ordering::SeqCst))?;

    tracing::info!("opening ZED camera at {FPS} fps (VGA, depth disabled)");
    let mut camera = ZedCamera::open(FPS)?;

    let node = NodeBuilder::new().create::<ipc::Service>()?;
    let service = open_image_service(&node)?;
    let publisher = service.publisher_builder().create()?;
    tracing::info!(service = zed_reader::IMAGE_SERVICE_NAME, "publishing");

    let mut frame_id: u64 = 0;
    let mut last_log_ts = std::time::Instant::now();

    while running.load(Ordering::SeqCst) {
        let mut sample = match publisher.loan_uninit() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "loan_uninit failed, retrying");
                continue;
            }
        };

        if let Err(e) = camera.grab_into(frame_id, sample.payload_mut()) {
            tracing::warn!(error = %e, "grab failed, retrying");
            continue;
        }

        // SAFETY: grab_into just fully initialized every field of the
        // payload (frame_id, timestamp_ns, width, height, data) or returned
        // Err above without reaching here.
        let sample = unsafe { sample.assume_init() };
        if let Err(e) = sample.send() {
            tracing::warn!(error = %e, "send failed");
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

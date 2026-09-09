//! Verification tool, not a reference consumer. Subscribes to the image
//! service and prints one summary line per frame.
use anyhow::Result;
use iceoryx2::prelude::*;
use zed_reader::{IMAGE_SERVICE_NAME, ZedFrame, open_service};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let node = NodeBuilder::new().create::<ipc::Service>()?;
    let service = open_service::<ZedFrame>(&node, IMAGE_SERVICE_NAME)?;
    let subscriber = service.subscriber_builder().create()?;
    tracing::info!(service = IMAGE_SERVICE_NAME, "subscribed");

    let mut last_ts: Option<u64> = None;
    let mut received = 0u64;
    let max_frames: u64 = std::env::var("DEBUG_SUB_MAX_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);

    while node.wait(std::time::Duration::from_millis(50)).is_ok() && received < max_frames {
        while let Some(sample) = subscriber.receive()? {
            let checksum: u64 = sample
                .data
                .iter()
                .step_by(4099) // sample, don't sum a megabyte every frame
                .map(|&b| b as u64)
                .sum();
            let dt_ms = last_ts
                .map(|prev| (sample.timestamp_ns.saturating_sub(prev)) as f64 / 1e6)
                .unwrap_or(0.0);
            last_ts = Some(sample.timestamp_ns);
            received += 1;
            tracing::info!(
                frame_id = sample.frame_id,
                width = sample.width,
                height = sample.height,
                dt_ms = format!("{dt_ms:.1}"),
                checksum,
                "received"
            );
        }
    }

    tracing::info!(received, "done");
    Ok(())
}

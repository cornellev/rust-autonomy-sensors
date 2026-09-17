use anyhow::Result;
use zed_reader::{IMAGE_KEY_EXPR, decode_image_frame, zenoh_config_from_env};
use zenoh::{Wait, handlers::RingChannel};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let session = zenoh::open(zenoh_config_from_env()?)
        .wait()
        .map_err(|error| anyhow::anyhow!("open Zenoh session: {error}"))?;
    let subscriber = session
        .declare_subscriber(IMAGE_KEY_EXPR)
        .with(RingChannel::new(4))
        .wait()
        .map_err(|error| anyhow::anyhow!("declare image subscriber: {error}"))?;
    tracing::info!(key_expr = IMAGE_KEY_EXPR, "subscribed");

    let mut last_ts: Option<u64> = None;
    let mut received = 0u64;
    let max_frames: u64 = std::env::var("DEBUG_SUB_MAX_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);

    while received < max_frames {
        let Some(sample) = subscriber
            .recv_timeout(std::time::Duration::from_millis(50))
            .map_err(|error| anyhow::anyhow!("receive image sample: {error}"))?
        else {
            continue;
        };
        let shared_memory = sample.payload().as_shm().is_some();
        let payload = sample.payload().to_bytes();
        let frame = decode_image_frame(&payload)?;
        let checksum: u64 = frame
            .data
            .iter()
            .step_by(4099)
            .map(|&byte| byte as u64)
            .sum();
        let dt_ms = last_ts
            .map(|prev| (frame.timestamp_ns.saturating_sub(prev)) as f64 / 1e6)
            .unwrap_or(0.0);
        last_ts = Some(frame.timestamp_ns);
        received += 1;
        tracing::info!(
            frame_id = frame.frame_id,
            width = frame.width,
            height = frame.height,
            dt_ms = format!("{dt_ms:.1}"),
            checksum,
            shared_memory,
            "received"
        );
    }

    tracing::info!(received, "done");
    Ok(())
}

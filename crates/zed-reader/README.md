# zed-reader

Owns the physical ZED camera and publishes image and depth through Zenoh using
explicitly allocated shared-memory payloads. Nothing else should call the ZED
SDK directly: the SDK does not allow two processes to open the same physical
device at once.

Same-host, SHM-capable Zenoh subscribers receive the original allocation.
Zenoh automatically copies the same payload across a network or other
non-SHM-capable link.

## Interface

Currently the only supported size is VGA (672x376), and only the left camera
is used. Both streams use the custom encoding
`application/vnd.cornellev.zed-frame`.

- **Image**: key expression `zed/zed2/image_left_vga_bgra`; body is
  `672*376*4` BGRA bytes.
- **Depth**: key expression `zed/zed2/depth_vga_f32`; body is
  `672*376` little-endian `f32` values in meters. NaN/inf marks a pixel with
  no valid depth, so consumers must check `is_finite()`.

Every payload is one contiguous allocation consisting of this 32-byte header
followed immediately by the body:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | ASCII magic `ZED1` |
| 4 | 1 | format version (`1`) |
| 5 | 1 | kind (`1` = BGRA image, `2` = f32 depth) |
| 6 | 2 | reserved, zero |
| 8 | 8 | `frame_id`, little-endian `u64` |
| 16 | 8 | ZED image timestamp in ns, little-endian `u64` |
| 24 | 4 | width, little-endian `u32` |
| 28 | 4 | height, little-endian `u32` |

Rust consumers can call `decode_image_frame` or `decode_depth_frame` on
`sample.payload().to_bytes()`. `to_bytes()` remains borrowed for the normal
contiguous SHM case. Consumers that must require local SHM rather than accept
Zenoh's network fallback should additionally check
`sample.payload().as_shm().is_some()`.

`frame_id` ties the image and depth streams to the same grab, but they are
published separately. Allocation or publication of one can fail while the
other succeeds. Join on `frame_id` and tolerate a missing counterpart.

## ZED neural depth

Depth (`sl::DEPTH_MODE::NEURAL` by default) is a GPU inference pass run every
frame. Set `ZED_READER_DEPTH_MODE` to `none`, `neural_light`, `neural`, or
`neural_plus` to change it. `none` skips the SDK depth pipeline and does not
declare the depth publisher.

Library callers select this with `ZedCamera::open(fps, DepthMode::None)`;
`camera.depth_enabled()` reports the result.

## Zenoh and SHM configuration

Zenoh's default peer configuration is used unless
`ZED_READER_ZENOH_CONFIG=/path/to/config.json5` is set. Use that file to
configure router endpoints, scouting, transport, or other deployment settings.

The publisher creates a 64 MiB POSIX SHM pool by default. Override it with
`ZED_READER_SHM_POOL_SIZE_MIB`. Subscriber-held samples retain their SHM
allocations, so consumers should use bounded/drop-old queues for this live
camera stream and release samples promptly.

Zenoh locks SHM pages. The process therefore needs a memlock limit at least as
large as the configured pool; for example, configure the service limit or run
the following in a suitably privileged shell:

```sh
ulimit -l unlimited
```

Containers must share the relevant `/dev/shm` mount to use SHM. Without a
shared SHM domain, Zenoh communication can still work but uses copied payloads.

## Building

The crate needs the ZED SDK (tested against CUDA 13.1 / `libsl_zed.so`) and
CUDA. It defaults to `/usr/local/zed` and `/usr/local/cuda`; override these
with `ZED_SDK_DIR` and `CUDA_DIR`.

This SDK install has no C API (`libsl_zed_c.so` and
`sl/c_api/zed_interface.h` are absent). `cpp/zed_camera.{h,cpp}` wraps the
small subset of `sl::Camera` needed here and is compiled by `build.rs`.

```sh
cargo build --release -p zed-reader
```

## Running

```sh
RUST_LOG=info ./target/release/zed-reader
```

Verify the image stream in another process:

```sh
RUST_LOG=info DEBUG_SUB_MAX_FRAMES=60 ./target/release/debug_sub
```

`debug_sub` uses a four-sample drop-old ring, validates and decodes the wire
format, and prints dimensions, inter-frame latency, a sparse checksum, and
`shared_memory=true|false`. It is a verification tool, not a reference
consumer.

## ZED SDK references

- [API reference](https://www.stereolabs.com/developers/documentation/API/)
- [Camera overview](https://docs.stereolabs.com/docs/development/zed-sdk/modules/camera)
- [Camera controls](https://docs.stereolabs.com/docs/development/zed-sdk/modules/camera/camera-controls)
- [zed-sdk on GitHub](https://github.com/stereolabs/zed-sdk)

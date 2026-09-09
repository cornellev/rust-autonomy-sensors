# zed-reader

Owns the physical ZED camera. Publishes image and depth over shared memory
([iceoryx2](https://github.com/eclipse-iceoryx/iceoryx2)). Nothing else should
call the ZED SDK directly: the SDK does not allow two processes to open the
same physical device at once.

## Interface

See ZED camera sources:

- [API Reference](https://www.stereolabs.com/developers/documentation/API/)
- [Camera Overview](https://docs.stereolabs.com/docs/development/zed-sdk/modules/camera)
- [Camera Controls](https://docs.stereolabs.com/docs/development/zed-sdk/modules/camera/camera-controls)
- [zed-sdk on GitHub](https://github.com/stereolabs/zed-sdk)

## Shared Memory Layout

Each of the ZED camera and ZED Neural Depth are published as `iceoryx2` publish-subscribe services in shared memory. Each payload is a
flat `#[repr(C)]` struct. Currently the only supported size is VGA (672x376), and only the left camera is used. This is planned to be expanded to both camera lenses in multiple possible output sizes (see https://github.com/cornellev/rust-autonomy-sensors/issues/4).
- **Image**: service `zed/zed2/image_left_vga_bgra`, payload `ZedFrame`:
  `frame_id: u64`, `timestamp_ns: u64`, `width: u32`, `height: u32`,
  `data: [u8; 672*376*4]` (BGRA, left camera).
- **Depth**: service `zed/zed2/depth_vga_f32`, payload `ZedDepthFrame`: same
  four header fields, `data: [f32; 672*376]` (meters). NaN/inf marks a pixel
  with no valid depth -- check `is_finite()` before use.

`frame_id` ties the two streams to the same grab, but they are sent
separately: a depth send can fail (and only log a warning) while its image
frame still goes out. Do not assume 1:1 delivery -- join on `frame_id` and
tolerate a missing depth frame for a given image.

## ZED Neural Depth
Depth (`sl::DEPTH_MODE::NEURAL` by default) is a GPU inference pass, run
every frame. Set `ZED_READER_DEPTH_MODE` to
`none`, `neural_light`, `neural`, or `neural_plus` to change it.
`none` skips the depth pipeline in the SDK call itself and `zed-reader` does
not create the depth service at all.

Library callers pick this the same way: `ZedCamera::open(fps, DepthMode::None)`
disables it; `camera.depth_enabled()` reports which.


## Building

Needs the ZED SDK (tested against CUDA 13.1 / `libsl_zed.so`) and CUDA.
Defaults to `/usr/local/zed` and `/usr/local/cuda`; override with
`ZED_SDK_DIR` / `CUDA_DIR` if yours differ.

This SDK install has no C API (`libsl_zed_c.so` and
`sl/c_api/zed_interface.h` are both absent, only the C++ `libsl_zed.so` is
present). `cpp/zed_camera.{h,cpp}` hand-wraps the `sl::Camera` calls this
crate needs, compiled by `build.rs` via the `cc` crate. A future SDK install
that does ship the C API would be a smaller, more official surface to bind
against instead.

```
cargo build --release -p zed-reader
```

## Running

```
RUST_LOG=info ./target/release/zed-reader
```

Verify the image stream end-to-end (separate process, real shared memory):

```
RUST_LOG=info DEBUG_SUB_MAX_FRAMES=60 ./target/release/debug_sub
```

`debug_sub` prints
frame dimensions, inter-frame latency, and a sparse pixel checksum per
frame, so you can confirm real (not static or garbage) frames arrive,
without printing a megabyte of pixel data per line.



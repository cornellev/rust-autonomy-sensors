# zed-reader

Owns the physical ZED camera. Publishes image and depth over shared memory
([iceoryx2](https://github.com/eclipse-iceoryx/iceoryx2)). Nothing else should
call the ZED SDK directly: the SDK does not allow two processes to open the
same physical device at once.

## Status

Image and depth capture over shared memory work. Validated against real ZED 2
hardware: `zed-reader` publishing, a separate `debug_sub` process subscribing
to the image stream, both against the real camera, live varying frame data,
zero grab failures over hundreds of frames. Depth values checked against real
hardware too (0.1-1.7m range, ~52% valid pixels, matched a close indoor
scene). Pose is not published yet.

## The interface

A concrete, single-sensor interface, not a standardized one. See the
top-level repo README for why. Depend on these directly:

- **Image**: service `zed/zed2/image_left_vga_bgra`, payload
  `zed_reader::ZedFrame` (`#[repr(C)]`): `frame_id: u64` (per-process counter,
  resets on restart), `timestamp_ns: u64` (ZED SDK clock,
  `sl::TIME_REFERENCE::IMAGE`), `width: u32`, `height: u32` (672x376 today),
  `data: [u8; 672*376*4]` (BGRA, left camera).
- **Depth**: service `zed/zed2/depth_vga_f32`, payload
  `zed_reader::ZedDepthFrame`: same four fields, `data: [f32; 672*376]`
  (meters; NaN/inf where the SDK has no valid depth for that pixel).

## Why VGA

Developed against real hardware over a WSL2 + USB/IP passthrough
(`usbipd-win`). The SDK's own diagnostic reported higher resolutions as
unreliable over that passthrough; `dmesg` showed repeated `vhci_hcd`
connection resets during a higher-resolution test. VGA had zero grab
failures over hundreds of frames. USB/IP may be the real constraint here, not
the SDK or the camera -- re-check at higher resolutions on the Jetson (no
USB/IP there).

## Building

Needs the ZED SDK (tested against CUDA 13.1 / `libsl_zed.so`) and CUDA.
Defaults to `/usr/local/zed` and `/usr/local/cuda`; override with
`ZED_SDK_DIR` / `CUDA_DIR` if yours differ.

This SDK install has no C API (`libsl_zed_c.so` and
`sl/c_api/zed_interface.h` are both absent, only the C++ `libsl_zed.so` is
present). `shim/zed_shim.{h,cpp}` hand-wraps the `sl::Camera` calls this
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

`debug_sub` is a verification tool, not a reference consumer. It prints
frame dimensions, inter-frame latency, and a sparse pixel checksum per
frame, so you can confirm real (not static or garbage) frames arrive,
without printing a megabyte of pixel data per line.

## What's next

- Publish pose.
- Re-check resolution limits on the Jetson, without USB/IP in the path.
- Calibration/extrinsics: no home yet, see top-level repo README.

# zed-reader

Owns the physical ZED camera exclusively and publishes the left image over
shared memory ([iceoryx2](https://github.com/eclipse-iceoryx/iceoryx2)) for
the rest of the stack to consume. Nothing else should call the ZED SDK
directly -- the SDK does not support multiple processes opening the same
physical device concurrently.

## Status

Image capture -> shared memory is implemented and has been validated against
real ZED 2 hardware: `zed-reader` publishing and a separate `debug_sub`
process subscribing, both against the real camera, with live varying frame
data (see git history / PR description for the session this came from).
Depth and pose are not published yet -- see "What's next" below.

## The interface

This is a concrete, single-sensor interface, not a standardized one -- see
the top-level repo README for why. If you're consuming ZED frames, depend on
`zed_reader::ZedFrame` and `zed_reader::IMAGE_SERVICE_NAME` directly.

- **Service name**: `zed/zed2/image_left_vga_bgra` (`iceoryx2`
  publish-subscribe).
- **Payload**: `zed_reader::ZedFrame` (`#[repr(C)]`) --
  `frame_id: u64` (per-process counter, not persisted across restarts),
  `timestamp_ns: u64` (the ZED SDK's own clock,
  `sl::TIME_REFERENCE::IMAGE`), `width: u32`, `height: u32` (both currently
  always 672x376), `data: [u8; 672*376*4]` (BGRA, left camera, no
  distortion/rectification beyond what the SDK does by default).

## Why VGA, why no depth (for now)

Developed against real camera hardware over a WSL2 + USB/IP passthrough
(`usbipd-win`), since that's what was available during initial development.
The SDK's own diagnostic tool reported higher resolutions as unreliable over
that specific passthrough (USB/IP does not reliably carry the sustained
high-bandwidth isochronous transfers full-resolution UVC video needs) --
confirmed by `dmesg` showing repeated `vhci_hcd` connection resets during a
higher-resolution test. VGA capture was validated with zero grab failures
over hundreds of frames. This may be a passthrough-specific limitation, not a
real constraint on the Jetson (no USB/IP in that path) -- re-validate at
higher resolutions once this runs there directly.

Depth is disabled (`sl::DEPTH_MODE::NONE`) in this first pass to keep scope to
"prove image capture -> real SHM end-to-end on real hardware." Depth doesn't
cost extra USB bandwidth (it's computed from the same stereo pair, not
transmitted separately), so adding a second published stream for it is a
straightforward follow-up, not a redesign.

## Building

Needs the ZED SDK (tested against a build with CUDA 13.1 / `libsl_zed.so`)
and CUDA installed. Defaults to `/usr/local/zed` and `/usr/local/cuda`;
override with the `ZED_SDK_DIR` / `CUDA_DIR` env vars if yours differ.

This SDK install has no C API (`libsl_zed_c.so` / `sl/c_api/zed_interface.h`
are absent, only the C++ `libsl_zed.so`) -- `shim/zed_shim.{h,cpp}` is a
small hand-written C-linkage wrapper around the handful of `sl::Camera` calls
this crate needs (`open`/`grab`/`retrieveImage`/`getTimestamp`/`close`),
compiled by `build.rs` via the `cc` crate. If a future SDK install does ship
the C API, that would be a smaller and more official surface to bind against
instead.

```
cargo build --release -p zed-reader
```

## Running

```
RUST_LOG=info ./target/release/zed-reader
```

Verify it end-to-end (separate process, real shared memory, no code shared
with the publisher beyond the `zed_reader` lib crate):

```
RUST_LOG=info DEBUG_SUB_MAX_FRAMES=60 ./target/release/debug_sub
```

`debug_sub` is a verification tool, not a reference consumer -- it prints
frame dimensions, inter-frame latency, and a sparse checksum of the pixel
data per frame so you can confirm real (not static/garbage) frames are
arriving, without printing a megabyte of pixel data per line.

## What's next

- Publish depth and pose alongside the image (separate `iceoryx2` services,
  or a combined payload -- not decided yet).
- Re-validate resolution limits on the Jetson directly, without USB/IP in the
  path.
- Calibration/extrinsics: no home yet, see top-level repo README.

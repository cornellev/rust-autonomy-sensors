# rust-autonomy-sensors

Home for the sensor-reading nodes that feed [rust-autonomy-stack](https://github.com/cornellev/rust-autonomy-stack),
Cornell EV's autonomy stack. Each sensor here is a standalone process that opens
one physical sensor and publishes its data over shared memory for the rest of
the stack to consume -- nothing in this repo
consumes another node's output, and nothing here talks to Zenoh directly.

## Why one repo for all of them

`rust-autonomy-stack` pulls in one repo per *role* (state estimation, planning,
...) via `gitman`, and expects each to be an independently versioned Cargo
package. Sensor readers could each get their own repo under that same model,
but we're deliberately not doing that yet:

- The shared publishing layer every sensor reader depends on is still being
  designed. While that's true, an interface change and updating every sensor
  that uses it should be one commit in one repo, not a coordinated version
  bump across N repos.
- Sensors should be able to be pulled in to different cars as they are needed,
  rather than being bundled all together -- this repo achieves this using
  standard Rust imports, but storing it all in one repo so it's not fragmented
  and sensor reading is not tied to the sensor consumers.

## Layout

```
rust-autonomy-sensors/
├── Cargo.toml         workspace root
├── crates/
│   ├── shm-common/    shared publish/subscribe plumbing used by every reader
│   ├── zed-reader/    ZED stereo camera -> rectified image + depth + pose
│   ├── imu-reader/    (future)
│   └── gps-reader/    (future)
└── README.md
```

Each `crates/<sensor>` package is meant to be pulled individually into
`rust-autonomy-stack` via a `gitman` source with
`links: - target: nodes/<role>`, the same mechanism already used there for
`rust-inekf` and `costmap-planner-rust` -- one crate here becomes one workspace
member there, selectable per car in `car.toml` (e.g. `cars/sim` need not run
`zed-reader` at all).

## Open questions

- Exact shape of the shared-memory publish/subscribe API in `shm-common`.
- Per-sensor/camera calibration and extrinsics (needed by anything projecting
  sensor data into 3D downstream) don't have a home yet -- same open question
  `rust-autonomy-stack`'s README raises for per-car hardware constants.

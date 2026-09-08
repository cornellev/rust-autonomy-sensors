# rust-autonomy-sensors

Home for the sensor-reading nodes that feed [rust-autonomy-stack](https://github.com/cornellev/rust-autonomy-stack),
Cornell EV's autonomy stack. Each sensor here is a standalone process that opens
one physical sensor and publishes its data over shared memory for the rest of
the stack to consume. The code in this repo assumes that no other process will
try to write into the shared memory that we claim, i.e. the memory is read-only
but not necessarily enforced as such.

`rust-autonomy-stack` pulls in one repo per *role* (state estimation, planning,
...) via `gitman`, and expects each to be an independently versioned Cargo
package. We are keeping this monorepo for our sensor reading packages rather
than separating them so that we can satisfy that structure by keeping the
shared memory interface standard across sensors. This way, if we change the
standard rust shm interface, it applies to all sensors at once.

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
`links: - target: nodes/<role>`. These can be selected per-car to match
the real sensors on the cars using `car.toml`.

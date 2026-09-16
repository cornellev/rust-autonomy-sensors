# rust-autonomy-sensors

Home for the sensor-reading nodes that feed [rust-autonomy-stack](https://github.com/cornellev/rust-autonomy-stack),
Cornell EV's autonomy stack. Each sensor here is a standalone process that opens
one physical sensor and publishes its data through Zenoh. Large sensor payloads
are allocated explicitly in Zenoh shared memory: same-host subscribers receive
them without a frame copy, while Zenoh transparently falls back to ordinary
network payloads for remote subscribers.

`rust-autonomy-stack` pulls in one repo per *role* (state estimation, planning,
...) via `gitman`, and expects each to be an independently versioned Cargo
package. We are keeping this monorepo for our sensor reading packages rather
than separating them so that we can satisfy that structure by keeping the
sensor transport and payload conventions consistent. This way, if we change
the Zenoh interface, it applies to all sensors at once.

## Layout

```
rust-autonomy-sensors/
├── Cargo.toml         workspace root
├── crates/
│   ├── zed-reader/    ZED stereo camera -> left image + depth (see its README)
│   ├── imu-reader/    (future)
│   └── gps-reader/    (future)
└── README.md
```
Documentation for each sensor is contained in separate `README.md` files within each crate.

There's no transport-common crate: each sensor defines its own concrete
publish/subscribe interface (Zenoh key expression + payload format) in its own crate,
per the "why one repo" rationale above -- see `crates/zed-reader/README.md`
for what that looks like in practice. Pull a shared plumbing crate out only
once a second or third sensor makes the duplication obviously real, rather
than guessing at a shared shape now.

Each `crates/<sensor>` package is meant to be pulled individually into
`rust-autonomy-stack` via a `gitman` source with
`links: - target: nodes/<role>`. These can be selected per-car to match
the real sensors on the cars using `car.toml`.

# j2534-bridge Agent Notes

This package is the shared CANsole J2534 bridge. It replaced the old CANcorder
`src-tauri/j2534-bridge` submodule.

Consumers:

- `apps/cancorder/src-tauri/Cargo.toml`
- `apps/cancorder/Makefile`
- `apps/studio/Makefile`
- `packages/python/pymulator/core/j2534.py`

The implementation is Windows-specific. Do not treat a plain macOS/Linux host
`cargo check` as the validation command. Use a Windows target:

```sh
rustup target add x86_64-pc-windows-msvc
cargo check --manifest-path packages/j2534-bridge/Cargo.toml --target x86_64-pc-windows-msvc
```

`make j2534-bridge-dev` from the repository root builds and stages development
bridge binaries for CANcorder.

## TODO: per-platform smoke test

The crate has 70 unit tests but they are all pure-Rust logic (JSON
round-trips, state-machine tables, name parsing). None exercise a real
OS API, and there is no `make smoke` that says "the binary you just
built can in fact start up on this OS." Add a `make smoke` target that
dispatches on `uname -s` and on each supported platform does something
cheap but real:

- **macOS**: `cargo test --lib`, then
  `cargo run --bin macos-can-usb-list`. Exercises the IOKit USB
  enumeration code in `src/macos.rs` (which currently has zero tests).
  Works without any CAN device attached — returns an empty list.
- **Linux**: `cargo test --lib`, then if `x86_64-pc-windows-gnu` or
  the MSVC linker is available,
  `cargo check --target x86_64-pc-windows-gnu` to confirm the
  cross-compile path used by CANcorder's Windows release still works.
- **Windows**: `cargo test --lib`, then
  `cargo run --bin j2534-dump -- --list`. Exercises the `winreg`
  J2534 DLL enumeration code. Works without any DLL installed —
  returns an empty list.

While adding this, drop the macOS refusal from `make test` — all 70
existing tests run fine there, the Makefile is just lying.

## CAN Ordering Notes

Some J2534 adapters can surface transmit echoes or driver-level receive timing
quirks. Keep adapter echo handling local to the bridge/runtime layer, but do not
use J2534-side filtering or UI sorting to hide internal runtime ordering bugs.
CAN observers must see causal order: for example `0x700 02 3E 00` before
`0x77A 02 7E 00`. If CANcorder shows responses before requests, check
ECUmulator virtual-bus reentrant dispatch and physical bridge self-echo
suppression before changing this package.

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

## Per-platform smoke test

`make smoke` dispatches on `uname -s` and runs something cheap but
real on each supported platform — enough to confirm the binaries
actually start up against real OS APIs:

- **macOS**: `cargo test --lib`, then `cargo run --bin
  macos-can-usb-list`. Exercises the IOKit USB enumeration code in
  `src/macos.rs`. Works without any CAN device attached — returns an
  empty list.
- **Linux**: `cargo test --lib`, then if the
  `x86_64-pc-windows-gnu` target is installed, `cargo check --target
  x86_64-pc-windows-gnu` to confirm the cross-compile path used by
  CANcorder's Windows release still works.
- **Windows**: `cargo test --lib`, then `cargo run --bin j2534-dump
  -- --list`. Exercises the `winreg` J2534 DLL enumeration code.
  Works without any DLL installed — returns an empty list.

`make test` (fmt + clippy + unit tests) now runs on macOS as well; the
old refusal was lying — all unit tests are pure-Rust logic and pass
there.

## CAN Ordering Notes

Some J2534 adapters can surface transmit echoes or driver-level receive timing
quirks. Keep adapter echo handling local to the bridge/runtime layer, but do not
use J2534-side filtering or UI sorting to hide internal runtime ordering bugs.
CAN observers must see causal order: for example `0x700 02 3E 00` before
`0x77A 02 7E 00`. If CANcorder shows responses before requests, check
ECUmulator virtual-bus reentrant dispatch and physical bridge self-echo
suppression before changing this package.

# j2534-bridge

[![CI](https://github.com/mickeyl/j2534-bridge/actions/workflows/ci.yml/badge.svg)](https://github.com/mickeyl/j2534-bridge/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)

Out-of-process J2534 PassThru bridge for cross-bitness DLL loading and fault isolation.

## What it does

J2534 PassThru is the standard API for automotive diagnostic adapters on Windows. Most adapters ship 32-bit DLLs, but modern applications are 64-bit. A 64-bit process cannot load a 32-bit DLL directly.

This crate solves that by running the J2534 DLL in a **separate bridge process** that matches the DLL's architecture. The host application communicates with the bridge over a named pipe using a simple JSON-RPC protocol.

### Benefits

- **Cross-bitness**: 64-bit app uses 32-bit J2534 DLLs (and vice versa) by spawning the matching bridge executable.
- **Fault isolation**: A buggy or crashing J2534 DLL kills only the bridge process, not your application. Panics inside DLL calls are caught via `catch_unwind`.
- **High-throughput capture**: The `read_messages_drain` command performs a tight drain loop inside the bridge (batch 256 messages, up to 64 iterations per poll), avoiding packet loss during passive CAN bus logging.
- **Full API coverage**: Connect, read, write, filters, periodic messages, IOCTL, version/voltage queries, loopback control, and configuration parameters.

## Architecture

```
+------------------+          named pipe          +------------------+
|   Host App       |  <--- JSON-RPC/newline --->  |  j2534-bridge    |
|  (any bitness)   |                              |  (matches DLL)   |
|                  |                              |                  |
|  BridgeClient    |                              |  J2534Connection |
|  (client.rs)     |                              |  (j2534.rs)      |
+------------------+                              +------------------+
                                                         |
                                                    LoadLibrary
                                                         |
                                                  +------v-------+
                                                  | J2534 DLL    |
                                                  | (vendor)     |
                                                  +--------------+
```

The host application uses `BridgeClient` to spawn the bridge process and send requests. The bridge loads the J2534 DLL, calls the PassThru API functions, and returns results over the pipe.

## Crate structure

The crate provides both a **library** (for host applications) and a **binary** (the bridge process).

| Module | Purpose |
|--------|---------|
| `protocol.rs` | Shared IPC types: `Request`, `Response`, `Message`, `CanMessage`, etc. |
| `client.rs` | `BridgeClient` — spawns the bridge, connects via named pipe, sends requests. |
| `j2534.rs` | `J2534Connection` — FFI wrapper for the PassThru API, device enumeration, drain-loop reader. |
| `main.rs` | Bridge executable entry point: named pipe server, JSON-RPC message loop. |

Host applications depend on the library for `client` and `protocol` modules:

```toml
[dependencies]
j2534-bridge = "0.1.0"
```

```rust
use j2534_bridge::client::BridgeClient;

let mut bridge = BridgeClient::new();
bridge.start(32)?;  // start 32-bit bridge
bridge.open(dll_path, 5 /* CAN */, 500000, 0x800 /* CAN_ID_BOTH */)?;

let messages = bridge.read_messages_drain(25, 256, 64)?;
for msg in &messages {
    println!("ID: {:X} Data: {:02X?}", msg.arb_id, msg.data);
}

bridge.close_connection()?;
bridge.stop()?;
```

Copy the built bridge executables to your application directory so the client can
launch the matching 32-bit or 64-bit helper at runtime.

## Building

The bridge must be compiled for each target architecture. Both executables should be placed alongside the host application:

```bash
# Build for both architectures
rustup target add x86_64-pc-windows-msvc i686-pc-windows-msvc
cargo build --release --target x86_64-pc-windows-msvc
cargo build --release --target i686-pc-windows-msvc

# Copy to your app directory
cp target/x86_64-pc-windows-msvc/release/j2534-bridge.exe /path/to/app/j2534-bridge-64.exe
cp target/i686-pc-windows-msvc/release/j2534-bridge.exe /path/to/app/j2534-bridge-32.exe
```

The `BridgeClient` searches for bridge executables in the following order:
1. Same directory as the host executable (`j2534-bridge-32.exe` / `j2534-bridge-64.exe`)
2. Development paths relative to `target/` (for `cargo run` workflows)

## Protocol

Communication uses newline-delimited JSON over a named pipe (`\\.\pipe\j2534-bridge-<pid>`).

Each message has an `id` field for request/response matching:

```json
{"id":1,"method":"Open","params":{"dll_path":"C:\\...\\passthru.dll","protocol_id":5,"baud_rate":500000,"connect_flags":2048}}
```

```json
{"id":1,"status":"ok","data":"Connected"}
```

### Connect flags

| Flag | Value | Description |
|------|-------|-------------|
| (none) | `0` | 11-bit CAN IDs only |
| `CAN_29BIT_ID` | `0x100` | 29-bit CAN IDs only |
| `CAN_ID_BOTH` | `0x800` | Both 11-bit and 29-bit (recommended for passive logging) |

### Supported commands

`EnumerateDevices`, `Open`, `Close`, `SendMessage`, `SendMessagesBatch`, `WriteMessagesRaw`, `ReadMessages` (with drain parameters), `ReadMessagesWithLoopback`, `ReadMessagesRaw`, `ClearBuffers`, `ReadVersion`, `GetLastError`, `ReadBatteryVoltage`, `ReadProgrammingVoltage`, `StartPeriodicMessage`, `StopPeriodicMessage`, `ClearPeriodicMessages`, `AddFilter`, `AddFilterRaw`, `RemoveFilter`, `ClearFilters`, `GetConfig`, `SetConfig`, `GetLoopback`, `SetLoopback`, `GetDataRate`, `Shutdown`.

## Test tool (`j2534-dump`)

The crate includes a CLI test harness for exercising the bridge against real hardware.

```bash
# List detected J2534 adapters
make list

# Mixed-mode capture (11-bit + 29-bit) with raw diagnostics
make dump DEVICE='OpenPort 2.0 J2534 ISO/CAN/VPW/PWM' BITNESS=32

# Loopback stress test — TX known frames and verify round-trip
make dump-stress-loopback DEVICE='OpenPort 2.0 J2534 ISO/CAN/VPW/PWM' BITNESS=32

# Push harder: 5000 frames, zero delay
make dump-stress-loopback DEVICE='...' BITNESS=32 EXTRA='--loopback-count 5000 --loopback-interval-ms 0'
```

### Available Makefile targets

| Target | Description |
|--------|-------------|
| `list` | Enumerate J2534 devices |
| `dump` | Mixed-mode capture with diagnostics |
| `dump-std` / `dump-ext` / `dump-both` | Standard / extended / both ID capture with filters |
| `dump-loopback` | Read with loopback echoes enabled |
| `dump-stress-loopback` | TX/RX loopback stress test with payload verification |
| `dump-raw` | Raw J2534 result codes |
| `dump-isotp` | ISO 15765 (ISO-TP) capture |

### Stress-loopback parameters

| CLI flag | Default | Description |
|----------|---------|-------------|
| `--loopback-count` | 100 | Number of frames to send |
| `--loopback-id` | `0x7DF` | Arbitration ID for TX frames |
| `--loopback-extended` | off | Use 29-bit extended IDs |
| `--loopback-interval-ms` | 10 | Delay between TX frames (0 = max throughput) |

## Known issues with selected adapters

### OBDX Pro FT — no 29-bit CAN receive

The OBDX Pro FT driver does not return 29-bit (extended) CAN frames in mixed-mode capture. This has been confirmed independently using SavvyCAN (Qt5-based J2534 tool) on the same bus and adapter — the issue is in the OBDX driver, not this bridge.

- `CAN_ID_BOTH` (`0x800`) alone fails to open on OBDX; `CAN_ID_BOTH | CAN_29BIT_ID` (`0x900`) is required.
- With `0x900`, the channel opens but only 11-bit frames are received.
- The same bridge code with a Tactrix OpenPort 2.0 on the same bus correctly receives both 11-bit and 29-bit traffic.

If you need 29-bit capture with OBDX, contact the vendor for a driver update.

## Diagnostics

Set `J2534_BRIDGE_VERBOSE=1` to log all requests and responses to stderr.

## Contributing

Run `cargo fmt` and `cargo clippy` before submitting a PR.

Add tests for new features.

Follow Rust best practices.

## License

MIT

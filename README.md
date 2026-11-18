# j2534-bridge

Out-of-process Windows CAN bridge with cross-bitness J2534 PassThru and a native gs_usb backend.

This crate is maintained as `packages/j2534-bridge` inside the CANsole monorepo.
It supersedes the former nested CANcorder submodule and is shared by CANcorder,
Studio, and the C++ desktop runtime. The repository also builds standalone
Windows helpers and diagnostic tools; it does not require a CANsole checkout.

## Version 1.1.0

- Raw Classic CAN and SAE J2534 CAN FD, including 64-byte payloads, BRS and ESI
  metadata, with both 32-bit and 64-bit vendor drivers.
- Native Windows gs_usb/candleLight access through libusb/WinUSB, without a
  vendor J2534 DLL. Discovery reports actual FD, listen-only and timestamp
  capabilities and preserves known devices while a capture owns WinUSB.
- A C ABI for native hosts, alongside the Rust client and named-pipe protocol.
- CANsole's Windows CANcorder picker and Studio runtime inventory/open paths
  integrate the native backend. Existing installed product releases need an
  updated application/runtime and matching bridge helpers to use it.

On the isolated PCAN-USB Pro FD / candleLightFD bench, 500 kbit/s arbitration
and 2 Mbit/s data with BRS passed bidirectional exact-frame and reopen tests.
**4 Mbit/s BRS is not accepted**: it failed on this Windows bench and required
USB power cycling to recover the candleLightFD. No general electrical silent-mode,
sustained-load or suspend/resume acceptance is implied.

See [the raw-CAN API and bench tools](docs/raw-can-fd.md) for exact contracts,
SAE versus GMW dialects, test commands and remaining limits.

## What it does

J2534 PassThru is the standard API for automotive diagnostic adapters on Windows. Most adapters ship 32-bit DLLs, but modern applications are 64-bit. A 64-bit process cannot load a 32-bit DLL directly.

This crate solves that by running the J2534 DLL in a **separate bridge process** that matches the DLL's architecture. The host application communicates with the bridge over a named pipe using a simple JSON-RPC protocol.

### Benefits

- **Cross-bitness**: 64-bit app uses 32-bit J2534 DLLs (and vice versa) by spawning the matching bridge executable.
- **Fault isolation**: A buggy or crashing J2534 DLL kills only the bridge process, not your application. Panics inside DLL calls are caught via `catch_unwind`.
- **High-throughput capture**: The `read_messages_drain` command performs a tight drain loop inside the bridge (batch 256 messages, up to 64 iterations per poll), avoiding packet loss during passive CAN bus logging.
- **J2534 API support**: Connect, read, write, filters, periodic messages, IOCTL, version/voltage queries, loopback control, and configuration parameters.

## Architecture

```text
Rust / C / C++ host
        |
        | named pipe: newline-delimited JSON requests and responses
        v
j2534-bridge-32.exe / j2534-bridge-64.exe
        |
        +-- J2534 backend --> vendor PassThru DLL --> CAN adapter
        |
        +-- native gs_usb --> libusb / WinUSB --> candleLight / compatible USB CAN
```

J2534 helpers match the vendor DLL's architecture. Native gs_usb normally uses
an x64 helper. Both backends share raw-frame send/read and lifecycle commands;
J2534-specific operations are rejected by the native backend. This is **not a
J2534 driver DLL** for arbitrary third-party PassThru applications.

Windows must already have a compatible USB driver for the gs_usb device
(the tested candleLightFD uses WinUSB). The bridge does not install drivers or
change driver bindings. It currently supports the first CAN channel only.
Discovery reads USB descriptors and capability bits without starting or resetting
CAN. If WinUSB denies an additional handle to a known, still-present adapter,
its previous discovery snapshot remains available until unplugged.

## Crate structure

The crate provides both a **library** (for host applications) and a **binary** (the bridge process).

| Module | Purpose |
|--------|---------|
| `protocol.rs` | Shared IPC types: `Request`, `Response`, `Message`, `CanMessage`, etc. |
| `client.rs` | `BridgeClient` — spawns the bridge, connects via named pipe, sends requests. |
| `j2534.rs` | `J2534Connection` — FFI wrapper for the PassThru API, device enumeration, drain-loop reader. |
| `gs_usb.rs` | Native USB discovery, controller timing, Classic/FD framing and device lifecycle. |
| `can.rs` | Shared raw-CAN options, exact-length validation and FD flag definitions. |
| `c_api.rs` | C ABI for native host clients; public header in `include/j2534_bridge.h`. |
| `worker.rs` | Isolated channel workers and bounded receive queues. |
| `main.rs` | Bridge executable entry point: named pipe server, JSON-RPC message loop. |

Host applications depend on the library for `client` and `protocol` modules. In
CANsole, use a path dependency:

```toml
[dependencies]
j2534-bridge = { path = "../../../packages/j2534-bridge" }
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

The bridge must be compiled for each Windows target architecture. Both
executables should be staged alongside the host application or into the Tauri
resource directory:

```bash
# From repository root
make j2534-bridge-dev
```

The `BridgeClient` searches for bridge executables in the following order:
1. Same directory as the host executable (`j2534-bridge-32.exe` / `j2534-bridge-64.exe`)
2. Tauri resource directory
3. `apps/cancorder/src-tauri/bin/` staged development resources
4. Development paths under `packages/j2534-bridge/target/`

Runtime helpers and the C ABI are Windows-specific. Portable library tests also
run on Linux/macOS; macOS additionally has a USB enumeration tool. Validate the
Windows implementation on Windows with both target architectures:

```bash
rustup target add x86_64-pc-windows-msvc i686-pc-windows-msvc
cargo test --all-targets --target x86_64-pc-windows-msvc
cargo test --all-targets --target i686-pc-windows-msvc
```

## Protocol

### Native C/C++ hosts

`include/j2534_bridge.h` exposes ABI v1 from the `j2534_bridge.lib` static library
(`cargo build --release --lib --target x86_64-pc-windows-msvc`). The C++ host
links this x64 library and stages both helper executables. `j2534_client_start`
selects the helper's bitness, not the host's. Existing Rust consumers still use
the same `rlib` and `BridgeClient` APIs.

Create a client with a bounded response timeout, start it, and pass UTF-8 JSON
Requests to `j2534_client_request`. Responses use the existing protocol's
`status`/`data`/`message` fields without the internal request-id envelope. Every
returned string belongs to Rust and must be released with `j2534_string_free`;
destroying the client stops and reaps the worker. Calls on one client must be
serialized. No vendor DLL is loaded into the host process. See the header for
null handling, ownership and pointer lifetime requirements.

`cargo test --lib --target x86_64-pc-windows-msvc` and the corresponding
`i686-pc-windows-msvc` command cover the ABI validation and packed v05 layout.
CANsole's C++ tests also exercise the ABI against both real helper processes
using registry enumeration only, without opening an adapter.

### Wire protocol

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

`EnumerateDevices`, `EnumerateNativeCanDevices`, `OpenCan`, `OpenNativeCan`, `SendCanFrames`, `Open`, `Close`, `SendMessage`, `SendMessagesBatch`, `WriteMessagesRaw`, `ReadMessages` (with drain parameters), `ReadMessagesWithLoopback`, `ReadMessagesRaw`, `ClearBuffers`, `ReadVersion`, `GetLastError`, `ReadBatteryVoltage`, `ReadProgrammingVoltage`, `StartPeriodicMessage`, `StopPeriodicMessage`, `ClearPeriodicMessages`, `AddFilter`, `AddFilterRaw`, `RemoveFilter`, `ClearFilters`, `GetConfig`, `SetConfig`, `GetLoopback`, `SetLoopback`, `GetDataRate`, `Shutdown`.

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

### Native gs_usb and CAN FD on Windows

```powershell
cargo run --bin j2534-dump -- --list-native
cargo run --bin j2534-dump -- --native 'gs_usb:1d50:606f:SERIAL' --can-fd --baud-rate 500000 --data-baud-rate 2000000 --duration-secs 5
```

Use the selector returned by discovery. Capture opens an ACK-capable controller;
add `--listen-only` for supported native devices when electrical silence is
required. J2534 listen-only is refused because disabling TX echo does not
suppress ACK. FD support is queried from the device, not assumed from its name.
No termination control, RTR, GMW17753 FD or ISO-TP FD is implemented here.

### Release binaries

[GitHub releases](https://github.com/mickeyl/j2534-bridge/releases) provide
Windows x64/x86 archives with the matching helper, `j2534-dump`, static C ABI
library, header and documentation. Stage both `j2534-bridge-32.exe` and
`j2534-bridge-64.exe` beside your host to use vendor DLLs of either architecture.
The archives include SHA-256 checksums; they are not Authenticode-signed installers.

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

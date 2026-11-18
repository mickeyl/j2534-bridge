# Raw CAN FD and native gs_usb

The bridge exposes a shared raw-CAN API over its existing named pipe and C ABI.
J2534 uses the vendor DLL in a helper of matching bitness. Native gs_usb uses
libusb/WinUSB in a helper; it does not install or pretend to be a vendor J2534 DLL.
Windows USB drivers are not changed by the bridge.

## API

- `EnumerateNativeCanDevices` returns `{ native_devices: [...] }`. Each entry has
  `selector`, `name`, `serial`, `backend`, `supports_can_fd`,
  `supports_listen_only` and `supports_hardware_timestamps`. Discovery reads USB
  descriptors and timing capability bits without claiming, resetting or starting
  the controller. Unknown capability bits default to false. Known devices retain
  their snapshot while an active WinUSB owner denies a second handle; unplugged
  devices leave the inventory. Opening validates the requested mode again.
- `OpenCan { dll_path, options }` selects raw Classic CAN or SAE `FD_CAN_PS`
  (`0x8011`). `options` contains `nominal_bitrate`, optional `data_bitrate`, optional
  vendor `device_name`, and `listen_only` (default false). Supplying `data_bitrate`
  enables FD. PEAK names can select a channel, e.g. `J2534-2:PEAK 0x51`.
- `OpenNativeCan { selector, options }` uses the same bitrate/access options.
  Omit `device_name`; the selector identifies the USB device. Currently the first
  gs_usb CAN channel is implemented. An absent serial is represented by `*`,
  accepted only when exactly one matching USB device is present.
- `SendCanFrames { frames, timeout_ms }` sends raw frames and returns the accepted
  count. A frame has `arb_id`, `data`, `extended`, `is_fd`, `bitrate_switch`.
  Validate exact wire lengths: Classic 0–8 bytes; FD additionally 12, 16, 20, 24,
  32, 48, 64. Never truncate or pad silently. BRS requires FD. ESI is controlled
  by the CAN controller, not by a transmit option. Counts mean accepted writes,
  not independent proof of an ACK. J2534 batches allow 256 frames; native gs_usb
  allows ten with separately tracked USB completion IDs.
- `ReadMessages`, `ReadVersion`, bitrate `GetConfig`/`GetDataRate`, `Close`, and
  `Shutdown` are shared. Native gs_usb explicitly rejects other J2534 operations.
  Native RX overflow, error packets, malformed packets, and USB failures surface
  as errors; reopening is required after a latched receive fault.

Rust clients use `open_can`, `enumerate_native_can_devices`, `open_native_can`,
`send_can_frames`, and the existing read methods. C/C++ clients send those same
JSON requests through ABI v1. Existing `Open`/`SendMessage` signatures remain;
raw Classic writes exceeding eight bytes now fail instead of being truncated.
The old send methods send Classic frames even on a newly opened FD channel.

Receive payloads retain every byte, `protocolId`, and `rxStatus`. For the SAE
FD protocol, status bits 20, 19, and 21 mean FD, BRS, and ESI respectively;
bit 8 preserves extended IDs even when their numeric value is below `0x800`.
Rust `CanMessage::{is_fd,bitrate_switch,error_state_indicator}` decodes these.
Classic frames on an FD channel remain Classic. Device timestamps are retained;
native gs_usb extends 32-bit timestamp wrap and uses a host clock when unavailable.

J2534 electrical listen-only is deliberately rejected: turning off loopback
does not disable ACK. Native gs_usb sets the controller listen-only flag only
when advertised and rejects transmission in that mode. Separate electrical
acceptance remains necessary. Neither path offers configurable termination here.

## Contracts and limits

PEAK API 04.04 uses the native `SCONFIG_LIST` layout. SAE FD setup connects,
sets data rate (`0x805C`), assigns J1962 pins (`0x8001`, value `0x060E`), then
installs standard/extended pass filters. API 05.00 passes pins plus data rate
as three connect resources and uses its packed configuration/message layouts.
The configured data rate is read back before success is reported.
`PassThruQueueMsgs` has three arguments (no timeout), whereas the legacy
`PassThruWriteMsgs` fallback has four. They must not share an x86 function
signature. Echo configuration uses `LOOPBACK` (0x03) in 04.04 and
`ECHO_PHYSICAL_CHANNEL_TX` (0x31) in 05.00. Normal raw-frame reception filters
driver-marked TX echoes; `SendCanFrames` does not populate the legacy
identity-based echo heuristic that can swallow an identical peer response.

GMW17753 `CAN_FD_PS` (`0x1000D`) is a different dialect and is not implemented.
ISO-TP FD is also outside this raw-frame API. Registry FD capability is not
proof that the selected dialect will open; unsupported DLLs return errors.
Inventory advertises raw SAE FD for the explicit `FD_CAN_PS` registry key.
PEAK's known API contract also permits it when 04.04 advertises only GMW FD
or 05.00 omits protocol keys. ISO-TP FD alone, or another vendor's GMW key,
does not enable this raw-SAE capability hint.

Native gs_usb reads controller timing limits and requires an exact bitrate.
The current timing profile targets 80% nominal/data sample points and 75% data
at 4 Mbit/s or higher, choosing a supported sample point at or below the target.
The observed legacy candleLightFD (40 MHz, feature mask `0x5B3`) advertises extended
timing constants but stalls that request. Only that identified profile uses the
conservative data-phase limits already used by CANsole's C++ backend.
Reset and stale USB queue draining precede starting the controller. USB TX
completion is not a physical ACK guarantee; sustained unacknowledged TX is not
accepted as a supported workload. RTR frames are reported as unsupported because
the existing common frame representation has no RTR field.

Authoritative references:
[PEAK 04.04 manual](https://www.peak-system.com/produktcd/Pdf/English/PCAN-PassThru-API_UserMan_eng.pdf),
[PEAK 05.00 manual](https://www.peak-system.com/produktcd/Pdf/English/PCAN-PassThru-API-Rev-05.00_UserMan_eng.pdf),
[Quantex configuration identifiers](https://www.scandoc.de/en/develop/j2534/pt_ioctl_config.html).

## Tools

Build from the package directory: `cargo build --bins --example can-fd-bench`.

```powershell
.\target\debug\j2534-dump.exe --list-native
.\target\debug\j2534-dump.exe --native 'gs_usb:1d50:606f:SERIAL' --can-fd --data-baud-rate 2000000 --duration-secs 5
.\target\debug\j2534-dump.exe --dll-path 'PATH\PCANPT32.dll' --open-name 'J2534-2:PEAK 0x51' --can-fd --data-baud-rate 2000000 --duration-secs 5
```

Capture opens an active ACK-capable controller unless native `--listen-only`
is supplied. `--send 123#0102` sends Classic; `--send 123##1AABBCCDDEEFF0011`
sends FD+BRS. Eight-digit IDs preserve extended format. Output uses candump
`##flags` for FD, including BRS and ESI. `--show-version` reports driver identity.

An explicitly authorized isolated bench with an independent FD-capable ACK peer
can run the reproducible acceptance example:

```powershell
.\scripts\test-can-fd-bench.ps1 -Dll 'PATH\PCANPT32.dll' -OpenName 'J2534-2:PEAK 0x51' -DataBitrate 2000000 -Cycles 3 -Transmit
```

The script builds/stages fresh helpers only beside the test example. `-Bitness 32`
tests a 32-bit vendor DLL from the 64-bit host with a 64-bit native peer. `-Classic`
tests Classic-only reopen. `-PeerFirst` reverses transmit order. Each FD cycle
checks 82 frames **in each direction**, all legal lengths, 11/29-bit IDs including
low-valued extended IDs, Classic-on-FD, FD without BRS and FD+BRS. Exact bytes,
flags and counts must match; extra frames and errors fail the test. No result is
inferred from successful opening, a write count, or local TX echo alone.

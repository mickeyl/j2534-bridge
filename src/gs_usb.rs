//! Native Windows gs_usb backend hosted by the same isolated bridge worker.
//! Wire contract mirrors CANsole's GsUsbDevice and candleLight firmware.
use crate::can::{CanOpenOptions, CanTxFrame, FD_CAN_BRS, FD_CAN_ESI, FD_CAN_FORMAT, FD_CAN_PS};
use crate::protocol::{CanMessage, NativeCanDeviceInfo};
use rusb::{DeviceHandle, GlobalContext};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(1);
const LENGTHS: [usize; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];
fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

pub struct GsUsb {
    handle: DeviceHandle<GlobalContext>,
    input: u8,
    output: u8,
    interface: u8,
    queued: Vec<u8>,
    timestamp: bool,
    started_at: Instant,
    last_timestamp: u32,
    timestamp_epoch: u64,
    fd: bool,
    listen_only: bool,
    pending_tx: [bool; 10],
    closed: bool,
}

impl GsUsb {
    pub fn open(selector: &str, options: &CanOpenOptions) -> Result<Self, String> {
        options.validate_rates()?;
        if options.device_name.is_some() {
            return Err(
                "Native CAN selects the device using selector; omit the vendor device_name".into(),
            );
        }
        let parts: Vec<_> = selector.split(':').collect();
        if parts.len() != 4 || parts[0] != "gs_usb" {
            return Err("Use gs_usb:VID:PID:SERIAL from EnumerateNativeCanDevices".into());
        }
        let vid = u16::from_str_radix(parts[1], 16).map_err(|e| e.to_string())?;
        let pid = u16::from_str_radix(parts[2], 16).map_err(|e| e.to_string())?;
        let nominal = options.nominal_bitrate;
        let data = options.data_bitrate;
        let devices = rusb::devices().map_err(|e| e.to_string())?;
        let candidates: Vec<_> = devices
            .iter()
            .filter(|d| {
                d.device_descriptor().is_ok_and(|v| {
                    v.vendor_id() == vid
                        && v.product_id() == pid
                        && (parts[3] == "*"
                            || d.open()
                                .and_then(|h| h.read_serial_number_string_ascii(&v))
                                .is_ok_and(|serial| serial == parts[3]))
                })
            })
            .collect();
        if candidates.len() != 1 {
            return Err(format!(
                "Expected exactly one gs_usb {vid:04x}:{pid:04x}; found {}",
                candidates.len()
            ));
        }
        let device = &candidates[0];
        let config = device
            .active_config_descriptor()
            .map_err(|e| e.to_string())?;
        let interface = config.interfaces().next().ok_or("No USB interface")?;
        let descriptor = interface
            .descriptors()
            .next()
            .ok_or("No USB interface descriptor")?;
        let endpoints: Vec<_> = descriptor
            .endpoint_descriptors()
            .filter(|e| e.transfer_type() == rusb::TransferType::Bulk)
            .collect();
        let input = endpoints
            .iter()
            .find(|e| e.direction() == rusb::Direction::In)
            .ok_or("No bulk-IN")?
            .address();
        let output = endpoints
            .iter()
            .find(|e| e.direction() == rusb::Direction::Out)
            .ok_or("No bulk-OUT")?
            .address();
        let handle = device.open().map_err(|e| {
            format!("Open gs_usb: {e}; a compatible Windows USB driver is required")
        })?;
        handle
            .claim_interface(descriptor.interface_number())
            .map_err(|e| e.to_string())?;
        let mut peer = Self {
            handle,
            input,
            output,
            interface: descriptor.interface_number(),
            queued: vec![],
            timestamp: false,
            started_at: Instant::now(),
            last_timestamp: 0,
            timestamp_epoch: 0,
            fd: data.is_some(),
            listen_only: options.listen_only,
            pending_tx: [false; 10],
            closed: false,
        };
        peer.control_at(0, 1, peer.interface as u16, &words(&[0xbeef]))?;
        peer.control(2, &words(&[0, 0]))?;
        let constants = peer.read_control(4, 40)?;
        let features = word(&constants, 0);
        peer.timestamp = features & 0x10 != 0;
        if options.listen_only && features & 1 == 0 {
            return Err("gs_usb does not support listen-only".into());
        }
        eprintln!(
            "gs_usb {vid:04x}:{pid:04x}: features=0x{features:X}, clock={} Hz",
            word(&constants, 4)
        );
        peer.set_timing(1, word(&constants, 4), &constants[8..40], nominal)?;
        if let Some(rate) = data {
            if features & 0x100 == 0 {
                return Err("gs_usb does not advertise CAN FD".into());
            }
            if features & 0x400 == 0 {
                return Err("Native gs_usb FD requires extended timing constants".into());
            }
            match peer.read_control(12, 72) {
                Ok(extended) => peer.set_timing(10, word(&extended, 4), &extended[40..72], rate)?,
                Err(error)
                    if (vid, pid, word(&constants, 4), features)
                        == (0x1d50, 0x606f, 40_000_000, 0x5b3) =>
                {
                    // This legacy candleLightFD advertises BT_CONST_EXT but stalls it.
                    // Same conservative data-phase limits as CANsole GsUsbDevice::fdFallbackBtConst.
                    eprintln!("legacy candleLightFD BT_CONST_EXT: {error}; using existing CANsole data-phase limits");
                    let limits = words(&[1, 32, 1, 16, 16, 1, 32, 1]);
                    peer.set_timing(10, word(&constants, 4), &limits, rate)?;
                }
                Err(error) => return Err(error),
            }
        }
        // Drop stale packets left by a previous session while the controller is reset.
        let mut stale = [0; 4096];
        for _ in 0..32 {
            if peer
                .handle
                .read_bulk(peer.input, &mut stale, Duration::from_millis(10))
                .is_err()
            {
                break;
            }
        }
        peer.control(
            2,
            &words(&[
                1,
                (if data.is_some() { 0x100 } else { 0 })
                    | if peer.timestamp { 0x10 } else { 0 }
                    | if options.listen_only { 1 } else { 0 },
            ]),
        )?;
        peer.queued.clear();
        Ok(peer)
    }

    fn control_at(&self, request: u8, value: u16, index: u16, bytes: &[u8]) -> Result<(), String> {
        let size = self
            .handle
            .write_control(0x41, request, value, index, bytes, TIMEOUT)
            .map_err(|e| e.to_string())?;
        if size != bytes.len() {
            return Err("Short gs_usb control write".into());
        }
        Ok(())
    }
    fn control(&self, request: u8, bytes: &[u8]) -> Result<(), String> {
        self.control_at(request, 0, 0, bytes)
    }
    fn read_control(&self, request: u8, size: usize) -> Result<Vec<u8>, String> {
        let mut bytes = vec![0; size];
        let received = self
            .handle
            .read_control(0xc1, request, 0, 0, &mut bytes, TIMEOUT)
            .map_err(|e| e.to_string())?;
        if received != size {
            return Err(format!("Short gs_usb capability read: {received}/{size}"));
        }
        Ok(bytes)
    }
    fn set_timing(
        &self,
        request: u8,
        clock: u32,
        limits: &[u8],
        bitrate: u32,
    ) -> Result<(), String> {
        let timing = calculate_timing(request, clock, limits, bitrate)?;
        eprintln!(
            "gs_usb timing {bitrate}: prop={} phase1={} phase2={} sjw={} brp={}",
            timing[0], timing[1], timing[2], timing[3], timing[4]
        );
        self.control(request, &words(&timing))
    }

    pub fn send(&mut self, frame: &CanTxFrame) -> Result<(), String> {
        if self.listen_only {
            return Err("Cannot transmit in listen-only mode".into());
        }
        frame.encode(self.fd)?;
        let echo = self
            .pending_tx
            .iter()
            .position(|pending| !pending)
            .ok_or("Native CAN TX queue full (10 pending USB completions)")?;
        let mut bytes = vec![0; if frame.is_fd { 76 } else { 20 }];
        bytes[..4].copy_from_slice(&(echo as u32).to_le_bytes());
        bytes[4..8].copy_from_slice(
            &(frame.arb_id | if frame.extended { 0x8000_0000 } else { 0 }).to_le_bytes(),
        );
        bytes[8] = LENGTHS
            .iter()
            .position(|n| *n == frame.data.len())
            .ok_or("Invalid wire length")? as u8;
        bytes[10] = if frame.is_fd {
            2 | if frame.bitrate_switch { 4 } else { 0 }
        } else {
            0
        };
        bytes[12..12 + frame.data.len()].copy_from_slice(&frame.data);
        let written = self
            .handle
            .write_bulk(self.output, &bytes, TIMEOUT)
            .map_err(|e| e.to_string())?;
        self.pending_tx[echo] = true;
        if written != bytes.len() {
            return Err("Short gs_usb bulk write".into());
        }
        Ok(())
    }

    pub fn receive(&mut self, timeout: Duration) -> Result<Vec<CanMessage>, String> {
        let deadline = Instant::now() + timeout;
        let mut frames = vec![];
        loop {
            let mut buffer = [0; 4096];
            match self
                .handle
                .read_bulk(self.input, &mut buffer, Duration::from_millis(2))
            {
                Ok(size) => self.queued.extend_from_slice(&buffer[..size]),
                Err(rusb::Error::Timeout) => {}
                Err(e) => return Err(e.to_string()),
            }
            while self.queued.len() >= 12 {
                let flags = self.queued[10];
                let is_fd = flags & 2 != 0;
                let size = (if is_fd { 76 } else { 20 }) + if self.timestamp { 4 } else { 0 };
                if self.queued.len() < size {
                    break;
                }
                let packet: Vec<_> = self.queued.drain(..size).collect();
                let echo = word(&packet, 0);
                if (echo as usize) < self.pending_tx.len() {
                    self.pending_tx[echo as usize] = false;
                }
                if let Some(mut frame) = decode_packet(&packet, self.timestamp, self.fd)? {
                    if self.timestamp {
                        let raw = frame.timestamp_us as u32;
                        if raw < self.last_timestamp && self.last_timestamp - raw > 0x8000_0000 {
                            self.timestamp_epoch += 1u64 << 32;
                        }
                        self.last_timestamp = raw;
                        frame.timestamp_us += self.timestamp_epoch;
                    } else {
                        frame.timestamp_us = self.started_at.elapsed().as_micros() as u64;
                    }
                    frames.push(frame);
                }
            }
            if !frames.is_empty() || Instant::now() >= deadline {
                return Ok(frames);
            }
        }
    }
}

impl GsUsb {
    pub fn close(&mut self) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        let reset = self.control(2, &words(&[0, 0]));
        let release = self
            .handle
            .release_interface(self.interface)
            .map_err(|e| e.to_string());
        self.closed = true;
        reset.and(release)
    }
}
impl Drop for GsUsb {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("gs_usb close failed: {error}");
        }
    }
}

/// Read USB descriptors and timing capability bits without claiming, resetting,
/// configuring or starting a CAN controller. Never write during discovery.
pub fn enumerate() -> Result<Vec<NativeCanDeviceInfo>, String> {
    // WinUSB denies a second handle while a worker owns the interface. Keep a
    // previously read identity/capability snapshot for that still-present USB
    // address; remove it as soon as the device disappears from enumeration.
    type UsbLocation = (u8, u8, u16, u16);
    static CACHE: OnceLock<Mutex<HashMap<UsbLocation, NativeCanDeviceInfo>>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|e| e.to_string())?;
    let mut present = HashMap::new();
    let mut result = vec![];
    for device in rusb::devices().map_err(|e| e.to_string())?.iter() {
        let descriptor = match device.device_descriptor() {
            Ok(d) => d,
            Err(_) => continue,
        };
        let vid = descriptor.vendor_id();
        let pid = descriptor.product_id();
        let location = (device.bus_number(), device.address(), vid, pid);
        if !matches!(
            (vid, pid),
            (0x1d50, 0x606f)
                | (0x1209, 0x2323)
                | (0x1cd2, 0x606f)
                | (0x16d0, 0x10b8)
                | (0x16d0, 0x0f30)
                | (0x1209, 0xca01)
                | (0x1d50, 0x606d)
                | (0x1d50, 0x6070)
                | (0x1209, 0x0001)
        ) {
            continue;
        }
        let handle = match device.open() {
            Ok(handle) => handle,
            Err(error) => {
                if matches!(error, rusb::Error::Access | rusb::Error::Busy) {
                    if let Some(previous) = cache.get(&location) {
                        present.insert(location, previous.clone());
                        result.push(previous.clone());
                        continue;
                    }
                }
                eprintln!("USB inventory {vid:04x}:{pid:04x}: {error}");
                continue; // One inaccessible device must not hide other adapters.
            }
        };
        let serial = handle
            .read_serial_number_string_ascii(&descriptor)
            .unwrap_or_default();
        let name = handle
            .read_product_string_ascii(&descriptor)
            .unwrap_or_else(|_| "gs_usb".into());
        let mut constants = [0; 40];
        let features = match handle.read_control(0xc1, 4, 0, 0, &mut constants, TIMEOUT) {
            Ok(40) => word(&constants, 0),
            _ => 0, // Do not infer FD or silent-mode support from a USB product ID.
        };
        let info = NativeCanDeviceInfo {
            selector: format!(
                "gs_usb:{vid:04x}:{pid:04x}:{}",
                if serial.is_empty() { "*" } else { &serial }
            ),
            name,
            serial,
            backend: "gs_usb".into(),
            supports_can_fd: features & 0x500 == 0x500,
            supports_listen_only: features & 1 != 0,
            supports_hardware_timestamps: features & 0x10 != 0,
        };
        present.insert(location, info.clone());
        result.push(info);
    }
    *cache = present;
    Ok(result)
}

fn calculate_timing(
    request: u8,
    clock: u32,
    limits: &[u8],
    bitrate: u32,
) -> Result<[u32; 5], String> {
    if limits.len() != 32 || clock == 0 || bitrate == 0 {
        return Err("Invalid timing constants/rate".into());
    }
    let [t1_min, t1_max, t2_min, t2_max, sjw_max, brp_min, brp_max, brp_inc]: [u32; 8] =
        std::array::from_fn(|i| word(limits, i * 4));
    let target = if request == 10 && bitrate >= 4_000_000 {
        750
    } else {
        800
    }; // PEAK PassThru timing profile.
    let mut best = None;
    for brp in (brp_min.max(1)..=brp_max.min(1024)).step_by(brp_inc.max(1) as usize) {
        let divisor = u64::from(bitrate) * u64::from(brp);
        if divisor == 0 || u64::from(clock) % divisor != 0 {
            continue;
        }
        let quanta = (u64::from(clock) / divisor) as u32;
        for t2 in t2_min.max(2)..=t2_max.min(512) {
            let Some(t1) = quanta.checked_sub(t2 + 1) else {
                continue;
            };
            if t1 < t1_min || t1 > t1_max {
                continue;
            }
            let sample = (1 + t1) * 1000 / quanta;
            if sample > target {
                continue;
            }
            let error = sample.abs_diff(target);
            if best.as_ref().map_or(true, |(score, _)| error <= *score) {
                best = Some((
                    error,
                    [t1 / 2, t1 - t1 / 2, t2, sjw_max.min(t2).max(1), brp],
                ));
            }
        }
    }
    let (_, timing) = best.ok_or_else(|| format!("No exact gs_usb timing for {bitrate}"))?;
    Ok(timing)
}

fn decode_packet(
    packet: &[u8],
    timestamp: bool,
    fd_channel: bool,
) -> Result<Option<CanMessage>, String> {
    if packet.len() < 12 {
        return Err("Short gs_usb header".into());
    }
    let flags = packet[10];
    let is_fd = flags & 2 != 0;
    let expected = (if is_fd { 76 } else { 20 }) + if timestamp { 4 } else { 0 };
    if packet.len() != expected {
        return Err("Invalid gs_usb packet size".into());
    }
    let id = word(packet, 4);
    if flags & 1 != 0 || id & 0x2000_0000 != 0 {
        return Err(format!("gs_usb overflow/error frame: {packet:02X?}"));
    }
    if word(packet, 0) != u32::MAX {
        return Ok(None);
    }
    let length = *LENGTHS
        .get(packet[8] as usize)
        .ok_or("Invalid gs_usb DLC")?;
    if (!is_fd && length > 8) || packet.len() < 12 + length {
        return Err("Invalid gs_usb frame length".into());
    }

    if id & 0x4000_0000 != 0 {
        return Err("RTR frames are not represented by the bridge CAN API".into());
    }
    if is_fd && !fd_channel {
        return Err("FD packet on a Classic CAN channel".into());
    }
    let timestamp_us = if timestamp {
        u64::from(word(packet, if is_fd { 76 } else { 20 }))
    } else {
        0
    };
    Ok(Some(CanMessage {
        arb_id: id & 0x1fff_ffff,
        extended: id & 0x8000_0000 != 0,
        data: packet[12..12 + length].to_vec(),
        timestamp_us,
        raw_arb_id: id & 0x1fff_ffff,
        data_size: (length + 4) as u32,
        protocol_id: if fd_channel { FD_CAN_PS } else { 5 },
        rx_status: (if id & 0x8000_0000 != 0 { 0x100 } else { 0 })
            | (if is_fd { FD_CAN_FORMAT } else { 0 })
            | (if flags & 4 != 0 { FD_CAN_BRS } else { 0 })
            | (if flags & 8 != 0 { FD_CAN_ESI } else { 0 }),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(fd: bool, dlc: u8, flags: u8, id: u32, timestamp: bool) -> Vec<u8> {
        let base = if fd { 76 } else { 20 };
        let mut packet = vec![0; base + if timestamp { 4 } else { 0 }];
        packet[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        packet[4..8].copy_from_slice(&id.to_le_bytes());
        packet[8] = dlc;
        packet[10] = flags | if fd { 2 } else { 0 };
        if timestamp {
            packet[base..].copy_from_slice(&0xfedcba98u32.to_le_bytes());
        }
        packet
    }

    #[test]
    fn fd_packet_preserves_64_bytes_low_extended_id_brs_esi_and_timestamp() {
        let mut wire = packet(true, 15, 4 | 8, 0x8000_0123, true);
        for (i, value) in wire[12..76].iter_mut().enumerate() {
            *value = i as u8;
        }
        let frame = decode_packet(&wire, true, true).unwrap().unwrap();
        assert_eq!(frame.data, (0..64).collect::<Vec<_>>());
        assert_eq!(frame.arb_id, 0x123);
        assert!(
            frame.extended
                && frame.is_fd()
                && frame.bitrate_switch()
                && frame.error_state_indicator()
        );
        assert_eq!(frame.timestamp_us, 0xfedcba98);
        assert_eq!(frame.data_size, 68);
    }

    #[test]
    fn classic_on_fd_channel_and_zero_length_fd_stay_distinct() {
        let classic = decode_packet(&packet(false, 0, 0, 0x123, false), false, true)
            .unwrap()
            .unwrap();
        assert!(!classic.is_fd());
        let fd = decode_packet(&packet(true, 0, 0, 0x123, false), false, true)
            .unwrap()
            .unwrap();
        assert!(fd.is_fd());
        assert!(fd.data.is_empty());
    }

    #[test]
    fn malformed_error_overflow_and_echo_packets() {
        assert!(decode_packet(&[0; 8], false, true).is_err());
        assert!(decode_packet(&packet(false, 9, 0, 0x123, false), false, true).is_err());
        assert!(decode_packet(&packet(true, 16, 0, 0x123, false), false, true).is_err());
        assert!(decode_packet(&packet(true, 15, 1, 0x123, false), false, true).is_err());
        assert!(decode_packet(&packet(false, 8, 0, 0x2000_0008, false), false, true).is_err());
        let mut echo = packet(true, 15, 4, 0x123, true);
        echo[..4].copy_from_slice(&3u32.to_le_bytes());
        assert!(decode_packet(&echo, true, true).unwrap().is_none());
    }

    #[test]
    fn exact_timing_respects_clock_and_controller_limits() {
        let limits = words(&[1, 32, 1, 16, 16, 1, 32, 1]);
        for rate in [500_000, 1_000_000, 2_000_000, 4_000_000] {
            let t = calculate_timing(10, 40_000_000, &limits, rate).unwrap();
            assert_eq!(40_000_000, rate * t[4] * (1 + t[0] + t[1] + t[2]));
            assert!(t[0] + t[1] <= 32 && t[2] <= 16 && t[3] <= t[2]);
        }
        assert!(calculate_timing(10, 40_000_000, &limits, 3_000_001).is_err());
        assert!(calculate_timing(10, 0, &limits, 500_000).is_err());
    }
}

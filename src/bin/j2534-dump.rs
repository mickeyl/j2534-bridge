#[cfg(not(windows))]
fn main() {
    eprintln!("j2534-dump is only supported on Windows");
    std::process::exit(1);
}

#[cfg(windows)]
mod app {
    use clap::{Parser, ValueEnum};
    use j2534_bridge::client::BridgeClient;
    use j2534_bridge::protocol::{CanMessage, DeviceInfo, RawIoResult};
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    const PASS_FILTER: u32 = 1;
    const BLOCK_FILTER: u32 = 2;
    const FLOW_CONTROL_FILTER: u32 = 3;
    const CAN_29BIT_ID: u32 = 0x100;
    const CAN_ID_BOTH: u32 = 0x800;
    const CAN_MIXED_CAPTURE_FLAGS: u32 = CAN_ID_BOTH | CAN_29BIT_ID;

    #[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
    enum ConnectMode {
        Standard,
        Extended,
        Both,
    }

    #[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
    enum ReadMode {
        Drain,
        Simple,
        Loopback,
        Raw,
        StressLoopback,
    }

    #[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
    enum TimestampMode {
        Delta,
        Relative,
        Absolute,
        None,
    }

    #[derive(Parser, Debug)]
    #[command(name = "j2534-dump")]
    #[command(about = "Exercise J2534 bridge open/filter/config parameters and dump traffic in a candump-style text format")]
    struct Cli {
        #[arg(long, help = "List devices visible through the selected bridge bitness and exit")]
        list: bool,

        #[arg(long, help = "Exact device name from registry enumeration")]
        device_name: Option<String>,

        #[arg(long, help = "Path to the vendor J2534 DLL")]
        dll_path: Option<String>,

        #[arg(long, help = "Bridge/J2534 DLL bitness: 32 or 64. Auto-detected from DLL when possible")]
        bitness: Option<u8>,

        #[arg(long, default_value_t = 5, help = "J2534 ProtocolID to open, e.g. 5=CAN, 6=ISO15765")]
        protocol_id: u32,

        #[arg(long, default_value_t = 500_000)]
        baud_rate: u32,

        #[arg(long, value_enum, default_value_t = ConnectMode::Both)]
        connect_mode: ConnectMode,

        #[arg(long, help = "Raw PassThruConnect Flags value. Overrides --connect-mode")]
        connect_flags: Option<String>,

        #[arg(long, help = "Call PassThruSetLoopback after connect")]
        set_loopback: Option<bool>,

        #[arg(long, help = "Clear TX/RX buffers after open and before capture")]
        clear_buffers: bool,

        #[arg(long, help = "Clear all message filters after open, before applying --filter entries")]
        clear_filters: bool,

        #[arg(long = "set-config", value_name = "PARAM=VALUE", help = "Repeatable J2534 SET_CONFIG parameter, decimal or 0x hex")]
        set_configs: Vec<String>,

        #[arg(long = "filter", value_name = "SPEC", help = "Repeatable filter: type:mask:pattern[:extended][:raw]. Example: pass:00000000:00000000:true:false")]
        filters: Vec<String>,

        #[arg(long, value_enum, default_value_t = ReadMode::Drain)]
        read_mode: ReadMode,

        #[arg(long, default_value_t = 25)]
        timeout_ms: u32,

        #[arg(long, default_value_t = 256)]
        batch_size: u32,

        #[arg(long, default_value_t = 64)]
        max_drain_reads: u32,

        #[arg(long, help = "Stop after this many seconds")]
        duration_secs: Option<f64>,

        #[arg(long, help = "Stop after this many frames")]
        max_messages: Option<u64>,

        #[arg(long, default_value = "j2534")]
        interface: String,

        #[arg(long, value_enum, default_value_t = TimestampMode::Delta)]
        timestamp: TimestampMode,

        #[arg(long, help = "Append printable ASCII like candump -a")]
        ascii: bool,

        #[arg(long, help = "Append raw J2534 fields (rx_status, data_size, raw arbitration field)")]
        raw_details: bool,

        #[arg(long, help = "Print device API/DLL/firmware version after open")]
        show_version: bool,

        #[arg(long, help = "Print battery and programming voltage after open")]
        show_voltage: bool,

        #[arg(long, help = "Print DATA_RATE and current loopback state after open")]
        show_state: bool,

        #[arg(long, default_value_t = 100, help = "Number of frames to send in stress-loopback mode")]
        loopback_count: u32,

        #[arg(long, default_value = "0x7DF", help = "Arbitration ID for stress-loopback TX frames")]
        loopback_id: String,

        #[arg(long, help = "Use 29-bit extended ID for stress-loopback TX frames")]
        loopback_extended: bool,

        #[arg(long, default_value_t = 10, help = "Delay in milliseconds between stress-loopback TX frames")]
        loopback_interval_ms: u64,
    }

    #[derive(Debug)]
    struct FilterSpec {
        filter_type: u32,
        mask: Vec<u8>,
        pattern: Vec<u8>,
        extended: bool,
        raw: bool,
    }

    pub fn run() -> Result<(), String> {
        let cli = Cli::parse();
        let initial_bitness = resolve_initial_bitness(&cli)?;

        let mut bridge = BridgeClient::new();
        bridge.start(initial_bitness)?;

        if cli.list {
            let devices = bridge.enumerate_devices()?;
            print_devices(&devices);
            return Ok(());
        }

        let selected = select_device(&mut bridge, &cli)?;
        let bitness = cli.bitness.unwrap_or(selected.bitness);
        if bitness != initial_bitness {
            bridge.stop()?;
            bridge.start(bitness)?;
        }
        let connect_flags = resolve_connect_flags(&cli)?;
        let filter_specs = cli
            .filters
            .iter()
            .map(|s| parse_filter_spec(s))
            .collect::<Result<Vec<_>, _>>()?;

        eprintln!(
            "[j2534-dump] opening dll={} bitness={} protocol={} baud={} flags=0x{:X}",
            selected.dll_path, bitness, cli.protocol_id, cli.baud_rate, connect_flags
        );
        bridge.open(
            &selected.dll_path,
            cli.protocol_id,
            cli.baud_rate,
            connect_flags,
        )?;

        if cli.clear_buffers {
            bridge.clear_buffers()?;
        }

        if cli.clear_filters {
            bridge.clear_filters()?;
        }

        if let Some(enabled) = cli.set_loopback {
            bridge.set_loopback(enabled)?;
        }

        for spec in &cli.set_configs {
            let (parameter, value) = parse_key_value_pair(spec)?;
            bridge.set_config(parameter, value)?;
            eprintln!(
                "[j2534-dump] set-config param=0x{:X} value=0x{:X}",
                parameter, value
            );
        }

        for spec in &filter_specs {
            let filter_id = if spec.raw {
                bridge.add_filter_raw(spec.filter_type, &spec.mask, &spec.pattern, spec.extended)?
            } else {
                bridge.add_filter(spec.filter_type, &spec.mask, &spec.pattern, spec.extended)?
            };
            eprintln!(
                "[j2534-dump] filter id={} type={} extended={} raw={} mask={} pattern={}",
                filter_id,
                filter_type_name(spec.filter_type),
                spec.extended,
                spec.raw,
                format_bytes_compact(&spec.mask),
                format_bytes_compact(&spec.pattern)
            );
        }

        if cli.show_version {
            let version = bridge.read_version()?;
            eprintln!(
                "[j2534-dump] version api={} dll={} fw={}",
                version.api_version, version.dll_version, version.firmware_version
            );
        }

        if cli.show_voltage {
            let vbatt = bridge.read_battery_voltage()?;
            let vprog = bridge.read_programming_voltage()?;
            eprintln!("[j2534-dump] voltages vbatt={vbatt:.3}V vprog={vprog:.3}V");
        }

        if cli.show_state {
            let loopback = bridge.get_loopback()?;
            let data_rate = bridge.get_data_rate()?;
            eprintln!(
                "[j2534-dump] state loopback={} data_rate={}",
                loopback, data_rate
            );
        }

        capture_loop(&mut bridge, &cli)
    }

    fn install_ctrlc_flag() -> Result<Arc<AtomicBool>, String> {
        let running = Arc::new(AtomicBool::new(true));
        let running_ctrlc = Arc::clone(&running);
        ctrlc::set_handler(move || {
            running_ctrlc.store(false, Ordering::SeqCst);
        })
        .map_err(|e| format!("Failed to install Ctrl+C handler: {e}"))?;
        Ok(running)
    }

    fn capture_loop(bridge: &mut BridgeClient, cli: &Cli) -> Result<(), String> {
        let running = install_ctrlc_flag()?;

        let start_monotonic = Instant::now();
        let start_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("System clock error: {e}"))?;
        let start_epoch_us = start_epoch.as_micros() as u64;
        let duration_limit = cli
            .duration_secs
            .map(Duration::from_secs_f64);
        let mut captured = 0u64;

        while running.load(Ordering::SeqCst) {
            if let Some(limit) = duration_limit {
                if start_monotonic.elapsed() >= limit {
                    break;
                }
            }
            if let Some(limit) = cli.max_messages {
                if captured >= limit {
                    break;
                }
            }

            match cli.read_mode {
                ReadMode::Drain => {
                    let messages = bridge.read_messages_drain(
                        cli.timeout_ms,
                        cli.batch_size,
                        cli.max_drain_reads,
                    )?;
                    let remaining = cli.max_messages.map(|limit| limit.saturating_sub(captured));
                    captured += print_messages(
                        &messages,
                        cli,
                        start_monotonic,
                        start_epoch_us,
                        captured,
                        remaining,
                        cli.raw_details,
                    );
                }
                ReadMode::Simple => {
                    let messages = bridge.read_messages(cli.timeout_ms)?;
                    let remaining = cli.max_messages.map(|limit| limit.saturating_sub(captured));
                    captured += print_messages(
                        &messages,
                        cli,
                        start_monotonic,
                        start_epoch_us,
                        captured,
                        remaining,
                        cli.raw_details,
                    );
                }
                ReadMode::Loopback => {
                    let messages = bridge.read_messages_with_loopback(cli.timeout_ms)?;
                    let remaining = cli.max_messages.map(|limit| limit.saturating_sub(captured));
                    captured += print_messages(
                        &messages,
                        cli,
                        start_monotonic,
                        start_epoch_us,
                        captured,
                        remaining,
                        cli.raw_details,
                    );
                }
                ReadMode::Raw => {
                    let raw = bridge.read_messages_raw(cli.timeout_ms, cli.batch_size)?;
                    print_raw_result(&raw);
                }
                ReadMode::StressLoopback => {
                    return stress_loopback(bridge, cli, &running);
                }
            }
        }

        eprintln!("[j2534-dump] captured {} frame(s)", captured);
        Ok(())
    }

    fn stress_loopback(bridge: &mut BridgeClient, cli: &Cli, running: &Arc<AtomicBool>) -> Result<(), String> {
        let tx_id = parse_u32(&cli.loopback_id)?;
        let extended = cli.loopback_extended;
        let count = cli.loopback_count;
        let interval = Duration::from_millis(cli.loopback_interval_ms);

        eprintln!(
            "[stress-loopback] sending {} frames, id=0x{:X}, extended={}, interval={}ms",
            count,
            tx_id,
            extended,
            cli.loopback_interval_ms
        );

        // Enable loopback
        bridge.set_loopback(true)?;
        bridge.clear_buffers()?;

        let start = Instant::now();
        let mut sent = 0u32;
        let mut received = 0u32;
        let mut matched = 0u32;
        let mut mismatched = 0u32;
        let mut bus_frames = 0u64;
        let mut pending: std::collections::VecDeque<(u32, [u8; 8])> = std::collections::VecDeque::new();

        // Marker byte so we can distinguish our frames from bus traffic
        const MARKER: u8 = 0xA5;

        for seq in 0..count {
            if !running.load(Ordering::SeqCst) {
                eprintln!("[stress-loopback] interrupted at seq={}", seq);
                break;
            }

            // Build payload: [MARKER, seq(4 bytes big-endian), 0, 0, 0]
            let mut payload = [0u8; 8];
            payload[0] = MARKER;
            payload[1] = (seq >> 24) as u8;
            payload[2] = (seq >> 16) as u8;
            payload[3] = (seq >> 8) as u8;
            payload[4] = seq as u8;

            bridge.send_message(tx_id, &payload, extended)?;
            sent += 1;
            pending.push_back((seq, payload));

            // Read back with short timeout
            let messages = bridge.read_messages_with_loopback(cli.timeout_ms)?;
            for msg in &messages {
                if msg.arb_id == tx_id && !msg.data.is_empty() && msg.data[0] == MARKER {
                    received += 1;
                    // Extract seq from echoed payload
                    let echo_seq = if msg.data.len() >= 5 {
                        ((msg.data[1] as u32) << 24)
                            | ((msg.data[2] as u32) << 16)
                            | ((msg.data[3] as u32) << 8)
                            | (msg.data[4] as u32)
                    } else {
                        u32::MAX
                    };

                    // Check against pending
                    let found = pending.iter().position(|(s, _)| *s == echo_seq);
                    if let Some(idx) = found {
                        let (_, expected_payload) = pending.remove(idx).unwrap();
                        if msg.data.len() == 8 && msg.data[..] == expected_payload[..] {
                            matched += 1;
                        } else {
                            mismatched += 1;
                            eprintln!(
                                "[stress-loopback] MISMATCH seq={} expected={:02X?} got={:02X?}",
                                echo_seq, &expected_payload[..], &msg.data[..]
                            );
                        }
                    } else {
                        mismatched += 1;
                        eprintln!(
                            "[stress-loopback] UNEXPECTED echo seq={} (not in pending)",
                            echo_seq
                        );
                    }
                } else {
                    bus_frames += 1;
                }
            }

            if interval > Duration::ZERO {
                std::thread::sleep(interval);
            }
        }

        // Final drain: collect any remaining loopback echoes
        let drain_deadline = Instant::now() + Duration::from_millis(500);
        while !pending.is_empty() && Instant::now() < drain_deadline {
            let messages = bridge.read_messages_with_loopback(50)?;
            if messages.is_empty() {
                break;
            }
            for msg in &messages {
                if msg.arb_id == tx_id && !msg.data.is_empty() && msg.data[0] == MARKER {
                    received += 1;
                    let echo_seq = if msg.data.len() >= 5 {
                        ((msg.data[1] as u32) << 24)
                            | ((msg.data[2] as u32) << 16)
                            | ((msg.data[3] as u32) << 8)
                            | (msg.data[4] as u32)
                    } else {
                        u32::MAX
                    };
                    let found = pending.iter().position(|(s, _)| *s == echo_seq);
                    if let Some(idx) = found {
                        let (_, expected_payload) = pending.remove(idx).unwrap();
                        if msg.data.len() == 8 && msg.data[..] == expected_payload[..] {
                            matched += 1;
                        } else {
                            mismatched += 1;
                        }
                    } else {
                        mismatched += 1;
                    }
                } else {
                    bus_frames += 1;
                }
            }
        }

        let lost = sent - received;
        let elapsed = start.elapsed();

        eprintln!();
        eprintln!("[stress-loopback] === RESULTS ===");
        eprintln!("[stress-loopback] elapsed:    {:.3}s", elapsed.as_secs_f64());
        eprintln!("[stress-loopback] sent:       {}", sent);
        eprintln!("[stress-loopback] received:   {}", received);
        eprintln!("[stress-loopback] matched:    {}", matched);
        eprintln!("[stress-loopback] mismatched: {}", mismatched);
        eprintln!("[stress-loopback] lost:       {}", lost);
        eprintln!("[stress-loopback] bus_frames: {} (non-loopback traffic seen)", bus_frames);
        if sent > 0 {
            eprintln!(
                "[stress-loopback] success:   {:.1}%",
                matched as f64 / sent as f64 * 100.0
            );
            eprintln!(
                "[stress-loopback] throughput: {:.1} frames/sec",
                sent as f64 / elapsed.as_secs_f64()
            );
        }

        if lost > 0 || mismatched > 0 {
            eprintln!("[stress-loopback] FAIL — frames were lost or corrupted");
        } else {
            eprintln!("[stress-loopback] PASS — all frames round-tripped correctly");
        }

        Ok(())
    }

    fn print_messages(
        messages: &[CanMessage],
        cli: &Cli,
        start_monotonic: Instant,
        start_epoch_us: u64,
        captured_before: u64,
        remaining: Option<u64>,
        raw_details: bool,
    ) -> u64 {
        let mut printed = 0u64;
        for (index, msg) in messages.iter().enumerate() {
            if remaining.is_some_and(|limit| printed >= limit) {
                break;
            }
            let line = format_candump_line(
                msg,
                &cli.interface,
                cli.timestamp,
                cli.ascii,
                start_monotonic,
                start_epoch_us,
                captured_before + index as u64,
                raw_details,
            );
            println!("{line}");
            printed += 1;
        }
        printed
    }

    fn print_raw_result(raw: &RawIoResult) {
        eprintln!(
            "[j2534-dump] raw result={} num_msgs={}",
            raw.result, raw.num_msgs
        );
    }

    fn format_candump_line(
        msg: &CanMessage,
        interface: &str,
        ts_mode: TimestampMode,
        ascii: bool,
        start_monotonic: Instant,
        start_epoch_us: u64,
        ordinal: u64,
        raw_details: bool,
    ) -> String {
        let prefix = match ts_mode {
            TimestampMode::None => String::new(),
            TimestampMode::Delta => format!("({:>12.6}) ", start_monotonic.elapsed().as_secs_f64()),
            TimestampMode::Relative => format!("({:>12.6}) ", msg.timestamp_us as f64 / 1_000_000.0),
            TimestampMode::Absolute => {
                let ts_us = start_epoch_us.saturating_add(msg.timestamp_us);
                format!("({:>12.6}) ", ts_us as f64 / 1_000_000.0)
            }
        };
        let id = if msg.extended {
            format!("{:08X}", msg.arb_id & 0x1FFF_FFFF)
        } else {
            format!("{:03X}", msg.arb_id & 0x7FF)
        };
        let data = format_bytes_compact(&msg.data);
        let mut line = format!("{prefix}{interface} {id}#{data}");
        if ascii {
            line.push_str("  ");
            line.push('\'');
            line.push_str(&ascii_render(&msg.data));
            line.push('\'');
        }
        if raw_details {
            line.push_str(&format!(
                "  [raw_arb=0x{:08X} rx_status=0x{:08X} data_size={}]",
                msg.raw_arb_id, msg.rx_status, msg.data_size
            ));
        }
        if matches!(ts_mode, TimestampMode::None) {
            line.push_str(&format!("  [{}]", ordinal));
        }
        line
    }

    fn ascii_render(data: &[u8]) -> String {
        data.iter()
            .map(|b| {
                if (0x20..=0x7E).contains(b) {
                    char::from(*b)
                } else {
                    '.'
                }
            })
            .collect()
    }

    fn print_devices(devices: &[DeviceInfo]) {
        for device in devices {
            println!(
                "{} | vendor={} | bitness={} | path={} | CAN={} | ISO15765={}",
                device.name,
                device.vendor,
                device.bitness,
                device.dll_path,
                device.can_iso11898,
                device.can_iso15765
            );
        }
    }

    fn select_device(bridge: &mut BridgeClient, cli: &Cli) -> Result<DeviceInfo, String> {
        if let Some(dll_path) = &cli.dll_path {
            let bitness = cli
                .bitness
                .or_else(|| detect_pe_bitness(dll_path).ok())
                .unwrap_or(64);
            return Ok(DeviceInfo {
                name: file_stem_label(dll_path),
                vendor: String::new(),
                dll_path: dll_path.clone(),
                can_iso15765: cli.protocol_id == 6,
                can_iso11898: cli.protocol_id == 5,
                compatible: true,
                bitness,
            });
        }

        let device_name = cli
            .device_name
            .as_ref()
            .ok_or_else(|| "Provide --device-name or --dll-path, or use --list".to_string())?;
        let devices = bridge.enumerate_devices()?;
        devices
            .into_iter()
            .find(|d| d.name == *device_name)
            .ok_or_else(|| format!("J2534 device not found: {device_name}"))
    }

    fn resolve_initial_bitness(cli: &Cli) -> Result<u8, String> {
        if let Some(bits) = cli.bitness {
            return validate_bitness(bits);
        }
        if let Some(dll_path) = &cli.dll_path {
            return detect_pe_bitness(dll_path)
                .map_err(|e| format!("Failed to detect DLL bitness for {dll_path}: {e}"));
        }
        Ok(64)
    }

    fn validate_bitness(bits: u8) -> Result<u8, String> {
        match bits {
            32 | 64 => Ok(bits),
            _ => Err(format!("Invalid bitness {bits}; expected 32 or 64")),
        }
    }

    fn resolve_connect_flags(cli: &Cli) -> Result<u32, String> {
        if let Some(raw) = &cli.connect_flags {
            return parse_u32(raw);
        }
        Ok(match cli.connect_mode {
            ConnectMode::Standard => 0,
            ConnectMode::Extended => CAN_29BIT_ID,
            ConnectMode::Both => CAN_MIXED_CAPTURE_FLAGS,
        })
    }

    fn parse_filter_spec(spec: &str) -> Result<FilterSpec, String> {
        let parts: Vec<&str> = spec.split(':').collect();
        if parts.len() < 3 || parts.len() > 5 {
            return Err(format!(
                "Invalid filter spec '{spec}'. Expected type:mask:pattern[:extended][:raw]"
            ));
        }
        let filter_type = match parts[0].to_ascii_lowercase().as_str() {
            "pass" => PASS_FILTER,
            "block" => BLOCK_FILTER,
            "flow" | "flow_control" => FLOW_CONTROL_FILTER,
            other => return Err(format!("Invalid filter type '{other}' in '{spec}'")),
        };
        let mask = parse_hex_bytes(parts[1])?;
        let pattern = parse_hex_bytes(parts[2])?;
        let extended = if parts.len() >= 4 {
            parse_bool(parts[3])?
        } else {
            false
        };
        let raw = if parts.len() >= 5 {
            parse_bool(parts[4])?
        } else {
            false
        };
        Ok(FilterSpec {
            filter_type,
            mask,
            pattern,
            extended,
            raw,
        })
    }

    fn parse_key_value_pair(spec: &str) -> Result<(u32, u32), String> {
        let (left, right) = spec
            .split_once('=')
            .ok_or_else(|| format!("Invalid PARAM=VALUE spec '{spec}'"))?;
        Ok((parse_u32(left)?, parse_u32(right)?))
    }

    fn parse_u32(input: &str) -> Result<u32, String> {
        let trimmed = input.trim();
        if let Some(hex) = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
        {
            u32::from_str_radix(hex, 16).map_err(|e| format!("Invalid hex integer '{input}': {e}"))
        } else {
            trimmed
                .parse::<u32>()
                .map_err(|e| format!("Invalid integer '{input}': {e}"))
        }
    }

    fn parse_bool(input: &str) -> Result<bool, String> {
        match input.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "y" | "on" => Ok(true),
            "0" | "false" | "no" | "n" | "off" => Ok(false),
            _ => Err(format!("Invalid boolean '{input}'")),
        }
    }

    fn parse_hex_bytes(input: &str) -> Result<Vec<u8>, String> {
        let cleaned: String = input
            .chars()
            .filter(|c| !matches!(c, ' ' | '_' | ':' | ',' | '-'))
            .collect();
        if cleaned.len() % 2 != 0 {
            return Err(format!("Hex byte string must have even length: '{input}'"));
        }
        let mut bytes = Vec::with_capacity(cleaned.len() / 2);
        for idx in (0..cleaned.len()).step_by(2) {
            let byte = u8::from_str_radix(&cleaned[idx..idx + 2], 16)
                .map_err(|e| format!("Invalid hex byte in '{input}': {e}"))?;
            bytes.push(byte);
        }
        Ok(bytes)
    }

    fn format_bytes_compact(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02X}")).collect()
    }

    fn filter_type_name(filter_type: u32) -> &'static str {
        match filter_type {
            PASS_FILTER => "pass",
            BLOCK_FILTER => "block",
            FLOW_CONTROL_FILTER => "flow_control",
            _ => "unknown",
        }
    }

    fn file_stem_label(path: &str) -> String {
        Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(path)
            .to_string()
    }

    fn detect_pe_bitness(path: &str) -> Result<u8, String> {
        use std::fs::File;
        use std::io::{Read, Seek, SeekFrom};

        let mut file = File::open(path).map_err(|e| format!("open failed: {e}"))?;

        let mut dos_header = [0u8; 64];
        file.read_exact(&mut dos_header)
            .map_err(|e| format!("read DOS header failed: {e}"))?;
        if &dos_header[0..2] != b"MZ" {
            return Err("not a PE file".to_string());
        }

        let pe_offset = u32::from_le_bytes([
            dos_header[0x3C],
            dos_header[0x3D],
            dos_header[0x3E],
            dos_header[0x3F],
        ]);
        file.seek(SeekFrom::Start(pe_offset as u64))
            .map_err(|e| format!("seek PE header failed: {e}"))?;

        let mut pe_header = [0u8; 6];
        file.read_exact(&mut pe_header)
            .map_err(|e| format!("read PE header failed: {e}"))?;
        if &pe_header[0..4] != b"PE\0\0" {
            return Err("invalid PE signature".to_string());
        }

        let machine = u16::from_le_bytes([pe_header[4], pe_header[5]]);
        match machine {
            0x014c => Ok(32),
            0x8664 | 0xAA64 => Ok(64),
            _ => Err(format!("unsupported PE machine 0x{machine:04X}")),
        }
    }
}

#[cfg(windows)]
fn main() {
    if let Err(err) = app::run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

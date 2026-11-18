//! Opt-in isolated-bus acceptance through the real helper IPC, with a native USB peer.

#[cfg(windows)]
mod app {
    use clap::Parser;
    use j2534_bridge::{
        can::{CanOpenOptions, CanTxFrame},
        client::BridgeClient,
    };
    use std::time::{Duration, Instant};

    #[derive(Parser)]
    #[command(
        version,
        about = "Isolated-bus J2534 acceptance with an independent gs_usb peer (native USB)"
    )]
    struct Args {
        #[arg(long)]
        dll: String,
        #[arg(long, default_value_t = 64)]
        bitness: u8,
        #[arg(long)]
        open_name: Option<String>,
        #[arg(long, default_value_t = 500_000)]
        nominal_bitrate: u32,
        /// Omit for Classic CAN regression.
        #[arg(long)]
        data_bitrate: Option<u32>,
        #[arg(long)]
        peer: Option<String>,
        #[arg(long, default_value_t = 3)]
        cycles: u32,
        /// Required acknowledgment: sends synthetic raw frames on an isolated bench.
        #[arg(long)]
        transmit: bool,
        #[arg(long)]
        peer_first: bool,
    }

    pub fn run() -> Result<(), String> {
        let args = Args::parse();
        if !args.transmit {
            return Err("Pass --transmit only on an authorized isolated, terminated bench".into());
        }
        if !matches!(args.bitness, 32 | 64) || !(1..=20).contains(&args.cycles) {
            return Err("Use bitness 32/64 and cycles 1..20".into());
        }
        let options = CanOpenOptions {
            nominal_bitrate: args.nominal_bitrate,
            data_bitrate: args.data_bitrate,
            device_name: args.open_name,
            listen_only: false,
        };
        options.validate()?;

        let mut bridge = BridgeClient::new();
        bridge.set_response_timeout(Duration::from_secs(10));
        bridge.start(args.bitness)?;
        let mut peer = BridgeClient::new();
        peer.set_response_timeout(Duration::from_secs(10));
        peer.start(64)?;
        let selector = match args.peer {
            Some(selector) => selector,
            None => {
                let devices = peer.enumerate_native_can_devices()?;
                if devices.len() != 1 {
                    return Err(format!(
                        "Select --peer from inventory; found {} devices",
                        devices.len()
                    ));
                }
                devices[0].selector.clone()
            }
        };
        for cycle in 0..args.cycles {
            bridge.open_can(&args.dll, options.clone())?;
            bridge.set_loopback(false)?;
            if bridge.get_loopback()? {
                return Err("Driver did not disable TX echo".into());
            }
            let mut peer_options = options.clone();
            peer_options.device_name = None;
            peer.open_native_can(&selector, peer_options)?;
            let version = bridge.read_version()?;
            eprintln!(
                "cycle {}: API={} DLL={} firmware={}",
                cycle + 1,
                version.api_version,
                version.dll_version,
                version.firmware_version
            );
            let mut frames = vec![];
            for extended in [false, true] {
                // Include low-valued extended IDs: width must come from metadata.
                for (is_fd, brs) in [(false, false), (true, false), (true, true)] {
                    if is_fd && options.data_bitrate.is_none() {
                        continue;
                    }
                    for len in [0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64] {
                        if !is_fd && len > 8 {
                            continue;
                        }
                        let ordinal = frames.len() as u32;
                        frames.push(CanTxFrame {
                            arb_id: if extended && ordinal % 2 == 0 {
                                0x18ff5000 + ordinal
                            } else {
                                0x520 + ordinal
                            },
                            extended,
                            is_fd,
                            bitrate_switch: brs,
                            data: (0..len)
                                .map(|i| {
                                    ((i * 37 + ordinal as usize * 11 + cycle as usize) & 255) as u8
                                })
                                .collect(),
                        });
                    }
                }
            }
            for (index, expected) in frames.iter().enumerate() {
                for from_peer in [args.peer_first, !args.peer_first] {
                    if from_peer {
                        if peer.send_can_frames(vec![expected.clone()], 1000)? != 1 {
                            return Err("Native peer accepted 0/1".into());
                        }
                        let deadline = Instant::now() + Duration::from_secs(2);
                        let mut received = vec![];
                        while received.is_empty() && Instant::now() < deadline {
                            received.extend(bridge.read_messages_drain(25, 256, 8)?);
                        }
                        if received.len() != 1 {
                            return Err(format!(
                                "gs_usb -> J2534 frame {index}: received {} instead of 1",
                                received.len()
                            ));
                        }
                        let actual = &received[0];
                        if actual.error_state_indicator() {
                            return Err("J2534 received ESI/error passive".into());
                        }
                        compare(
                            expected,
                            &CanTxFrame {
                                arb_id: actual.arb_id,
                                data: actual.data.clone(),
                                extended: actual.extended,
                                is_fd: actual.is_fd(),
                                bitrate_switch: actual.bitrate_switch(),
                            },
                            &format!(
                                "gs_usb -> J2534 frame {index}, raw status=0x{:X}",
                                actual.rx_status
                            ),
                        )?;
                    } else {
                        let count = bridge.send_can_frames(vec![expected.clone()], 1000)?;
                        if count != 1 {
                            return Err(format!("J2534 accepted {count}/1 at frame {index}"));
                        }
                        let received = peer.read_messages_drain(2000, 256, 8).map_err(|e| {
                            format!("J2534 -> gs_usb frame {index} {expected:?}: {e}")
                        })?;
                        if received.len() != 1 {
                            return Err(format!(
                                "J2534 -> gs_usb frame {index}: received {} instead of 1",
                                received.len()
                            ));
                        }
                        compare_message(
                            expected,
                            &received[0],
                            &format!("J2534 -> gs_usb frame {index}"),
                        )?;
                    }
                }
                // Drain the USB TX completion before reusing echo ID zero.
                if !peer.read_messages_drain(10, 256, 8)?.is_empty() {
                    return Err("Unexpected extra gs_usb frame".into());
                }
            }
            if !bridge.read_messages_drain(50, 256, 8)?.is_empty() {
                return Err("Unexpected trailing J2534 frames".into());
            }
            bridge.close_connection()?;
            peer.close_connection()?;
            println!("PASS cycle {}: {} frames in each direction, exact payload/ID/width/FD/BRS; nominal={} data={:?}", cycle + 1, frames.len(), options.nominal_bitrate, options.data_bitrate);
        }
        peer.stop()?;
        bridge.stop()?;
        Ok(())
    }

    fn compare_message(
        expected: &CanTxFrame,
        actual: &j2534_bridge::protocol::CanMessage,
        context: &str,
    ) -> Result<(), String> {
        if actual.error_state_indicator() {
            return Err(format!("{context}: error passive"));
        }
        compare(
            expected,
            &CanTxFrame {
                arb_id: actual.arb_id,
                data: actual.data.clone(),
                extended: actual.extended,
                is_fd: actual.is_fd(),
                bitrate_switch: actual.bitrate_switch(),
            },
            context,
        )
    }

    fn compare(expected: &CanTxFrame, actual: &CanTxFrame, context: &str) -> Result<(), String> {
        if expected.arb_id != actual.arb_id
            || expected.data != actual.data
            || expected.extended != actual.extended
            || expected.is_fd != actual.is_fd
            || expected.bitrate_switch != actual.bitrate_switch
        {
            return Err(format!("{context}: expected {expected:?}, got {actual:?}"));
        }
        Ok(())
    }
}

fn main() {
    #[cfg(windows)]
    if let Err(error) = app::run() {
        eprintln!("FAIL: {error}");
        std::process::exit(1);
    }
    #[cfg(not(windows))]
    {
        eprintln!("This bench requires Windows J2534");
        std::process::exit(1);
    }
}

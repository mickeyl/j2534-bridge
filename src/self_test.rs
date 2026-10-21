//! Bounded Classic-CAN TX-echo check. A J2534 echo is driver feedback, not
//! independent evidence of controller RX or correct vehicle wiring.
use crate::protocol::CanMessage;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub trait EchoChannel {
    fn enable_loopback(&mut self) -> Result<(), String>;
    fn send(&mut self, id: u32, data: &[u8]) -> Result<(), String>;
    fn read(&mut self, timeout_ms: u32) -> Result<Vec<CanMessage>, String>;
    fn close(&mut self) -> Result<(), String>;
}

pub fn test_echo(channel: &mut impl EchoChannel, nonce: u32) -> Value {
    let mut result = json!({"state": "failed", "mode": "tx_echo", "evidence": "echo",
        "sent": 0, "received": 0, "expected": 4, "cleanupOk": true, "detail": ""});
    let run = (|| -> Result<(), String> {
        channel.enable_loopback()?;
        for sequence in 0..4u8 {
            let mut payload = [b'C', b'A', b'N', 0, 0, 0, 0, sequence];
            payload[3..7].copy_from_slice(&nonce.to_le_bytes());
            channel.send(0x5A5, &payload)?;
            result["sent"] = json!(sequence + 1);
            let deadline = Instant::now() + Duration::from_millis(300);
            let mut matched = false;
            while Instant::now() < deadline {
                for frame in channel.read(25)? {
                    // TX_MSG_TYPE must be present. Never count an unrelated
                    // bus frame (or a TX indication without its payload).
                    if frame.rx_status & 1 != 0
                        && frame.protocol_id == 5
                        && frame.arb_id == 0x5A5
                        && !frame.extended
                        && frame.data == payload
                    {
                        matched = true;
                        break;
                    }
                }
                if matched {
                    break;
                }
            }
            if !matched {
                return Err("No matching CAN loopback frame received before timeout".into());
            }
            result["received"] = json!(sequence + 1);
        }
        Ok(())
    })();
    match run {
        Ok(()) => result["state"] = json!("passed"),
        Err(error) => result["detail"] = json!(error),
    }
    if let Err(error) = channel.close() {
        result["state"] = json!("failed");
        result["cleanupOk"] = json!(false);
        result["detail"] = json!(format!(
            "{} / CAN test cleanup failed: {}",
            result["detail"].as_str().unwrap_or(""),
            error
        ));
    }
    result
}

#[cfg(windows)]
pub fn run(dll_path: &str, bitness: u8, bitrate: u32) -> Value {
    use crate::client::BridgeClient;
    struct Channel(BridgeClient);
    impl EchoChannel for Channel {
        fn enable_loopback(&mut self) -> Result<(), String> {
            self.0.set_loopback(true)?;
            if !self.0.get_loopback()? {
                return Err("CAN loopback was not enabled by the driver".into());
            }
            self.0.clear_buffers()
        }
        fn send(&mut self, id: u32, data: &[u8]) -> Result<(), String> {
            self.0.send_message(id, data, false)
        }
        fn read(&mut self, timeout_ms: u32) -> Result<Vec<CanMessage>, String> {
            self.0.read_messages_with_loopback(timeout_ms)
        }
        fn close(&mut self) -> Result<(), String> {
            self.0.close_connection()
        }
    }
    let mut channel = Channel(BridgeClient::new());
    channel.0.set_response_timeout(Duration::from_secs(5));
    let opened = channel
        .0
        .start(bitness)
        .and_then(|()| channel.0.open(dll_path, 5, bitrate, 0));
    if let Err(error) = opened {
        // Drop terminates the dedicated worker, including partial opens.
        return json!({"state": "failed", "mode": "tx_echo", "evidence": "echo", "sent": 0,
            "received": 0, "expected": 4, "cleanupOk": true, "detail": error});
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos()
        ^ std::process::id();
    test_echo(&mut channel, nonce)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        last: Vec<u8>,
        closed: bool,
        enable_error: bool,
        corrupt: bool,
        echo: bool,
        close_error: bool,
    }
    impl Default for Fake {
        fn default() -> Self {
            Self {
                last: vec![],
                closed: false,
                enable_error: false,
                corrupt: false,
                echo: true,
                close_error: false,
            }
        }
    }
    impl EchoChannel for Fake {
        fn enable_loopback(&mut self) -> Result<(), String> {
            if self.enable_error {
                Err("unsupported".into())
            } else {
                Ok(())
            }
        }
        fn send(&mut self, _: u32, data: &[u8]) -> Result<(), String> {
            self.last = data.to_vec();
            Ok(())
        }
        fn read(&mut self, _: u32) -> Result<Vec<CanMessage>, String> {
            let mut data = self.last.clone();
            if self.corrupt {
                data[0] ^= 1;
            }
            std::thread::sleep(Duration::from_millis(1));
            Ok(vec![CanMessage {
                timestamp_us: 0,
                arb_id: 0x5A5,
                extended: false,
                data,
                raw_arb_id: 0x5A5,
                rx_status: u32::from(self.echo),
                data_size: 12,
                protocol_id: 5,
            }])
        }
        fn close(&mut self) -> Result<(), String> {
            self.closed = true;
            if self.close_error {
                Err("close".into())
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn matches_all_four_echoes_and_closes() {
        let mut fake = Fake::default();
        let result = test_echo(&mut fake, 123);
        assert_eq!(result["state"], "passed");
        assert_eq!(result["received"], 4);
        assert!(fake.closed);
    }
    #[test]
    fn rejects_corruption_and_unmarked_bus_frames() {
        for mut fake in [
            Fake {
                corrupt: true,
                ..Fake::default()
            },
            Fake {
                echo: false,
                ..Fake::default()
            },
        ] {
            let result = test_echo(&mut fake, 123);
            assert_eq!(result["state"], "failed");
            assert_eq!(result["received"], 0);
            assert!(fake.closed);
        }
    }
    #[test]
    fn unsupported_loopback_never_sends_and_cleanup_failure_never_passes() {
        let mut fake = Fake {
            enable_error: true,
            ..Fake::default()
        };
        let result = test_echo(&mut fake, 123);
        assert_eq!(result["sent"], 0);
        assert!(fake.closed);
        let mut fake = Fake {
            close_error: true,
            ..Fake::default()
        };
        let result = test_echo(&mut fake, 123);
        assert_eq!(result["state"], "failed");
        assert_eq!(result["cleanupOk"], false);
    }
}

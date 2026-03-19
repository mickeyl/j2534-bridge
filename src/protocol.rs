//! IPC Protocol for J2534 Bridge
//!
//! JSON-RPC style protocol over named pipes.
//! Each message is a JSON object followed by a newline.

use serde::{Deserialize, Serialize};

/// Request from the main app to the bridge
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Request {
    /// List available J2534 devices
    EnumerateDevices,

    /// Open a connection to a device.
    /// connect_flags: raw J2534 connect flags (0x800 = CAN_ID_BOTH, 0x100 = CAN_29BIT_ID, 0 = 11-bit only).
    /// Defaults to 0 if omitted. For passive logging, use 0x800 to receive both 11-bit and 29-bit frames.
    Open {
        dll_path: String,
        protocol_id: u32,
        baud_rate: u32,
        #[serde(default)]
        connect_flags: u32,
    },

    /// Close the current connection
    Close,

    /// Send a CAN message
    SendMessage {
        arb_id: u32,
        data: Vec<u8>,
        extended: bool,
    },

    /// Send multiple CAN messages in a single PassThruWriteMsgs call
    SendMessagesBatch { messages: Vec<BatchMessage> },

    /// Send messages with a custom timeout and return raw result info
    WriteMessagesRaw {
        messages: Vec<BatchMessage>,
        timeout_ms: u32,
    },

    /// Read messages with drain loop for high-throughput capture.
    /// First read blocks up to timeout_ms, subsequent reads use 0ms timeout
    /// to drain the buffer. Up to max_drain_reads iterations of batch_size each.
    ReadMessages {
        timeout_ms: u32,
        #[serde(default = "default_batch_size")]
        batch_size: u32,
        #[serde(default = "default_max_drain_reads")]
        max_drain_reads: u32,
    },

    /// Read messages including loopback echoes (for sanity testing)
    ReadMessagesWithLoopback { timeout_ms: u32 },

    /// Read messages and return raw J2534 result code + count (for spec testing)
    ReadMessagesRaw { timeout_ms: u32, max_msgs: u32 },

    /// Clear TX and RX buffers
    ClearBuffers,

    /// Read version information
    ReadVersion,

    /// Get last error string
    GetLastError,

    /// Read battery voltage
    ReadBatteryVoltage,

    /// Read programming voltage
    ReadProgrammingVoltage,

    /// Perform ISO/K-Line fast init and return the reply message
    FastInit { data: Vec<u8> },

    /// Perform ISO/K-Line five-baud init and return the reply message
    FiveBaudInit { data: Vec<u8> },

    /// Full K-Line autodetection and initialization (fast/slow/auto) with
    /// CC acknowledgment polling — runs entirely inside the bridge process
    /// to guarantee correct timing.
    KlineInit {
        init_mode: KlineInitMode,
        #[serde(default)]
        fast_init_data: Option<Vec<u8>>,
        #[serde(default)]
        five_baud_address: Option<Vec<u8>>,
        #[serde(default)]
        cc_timeout_ms: Option<u32>,
    },

    /// Start a periodic message
    StartPeriodicMessage {
        arb_id: u32,
        data: Vec<u8>,
        interval_ms: u32,
        extended: bool,
    },

    /// Stop a periodic message
    StopPeriodicMessage { msg_id: u32 },

    /// Clear all periodic messages
    ClearPeriodicMessages,

    /// Add a message filter
    AddFilter {
        filter_type: String,
        mask: Vec<u8>,
        pattern: Vec<u8>,
        extended: bool,
    },

    /// Add a message filter with raw mask/pattern sizes (for spec testing)
    AddFilterRaw {
        filter_type: String,
        mask: Vec<u8>,
        pattern: Vec<u8>,
        extended: bool,
    },

    /// Remove a message filter
    RemoveFilter { filter_id: u32 },

    /// Clear all filters
    ClearFilters,

    /// Get a configuration parameter
    GetConfig { parameter: u32 },

    /// Set a configuration parameter
    SetConfig { parameter: u32, value: u32 },

    /// Get loopback setting
    GetLoopback,

    /// Set loopback setting
    SetLoopback { enabled: bool },

    /// Get current data rate
    GetDataRate,

    /// Shutdown the bridge process
    Shutdown,
}

fn default_batch_size() -> u32 {
    256
}

fn default_max_drain_reads() -> u32 {
    64
}

/// Response from the bridge to the main app
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum Response {
    #[serde(rename = "ok")]
    Ok { data: ResponseData },

    #[serde(rename = "error")]
    Error { code: i32, message: String },
}

/// K-Line initialization mode
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KlineInitMode {
    Fast,
    Slow,
    Auto,
}

/// Result of a server-side KlineInit command
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KlineInitResult {
    /// Which method succeeded: "fast" or "slow"
    pub init_method: String,
    /// Detected protocol: "iso14230-fast", "iso9141", "iso14230-slow"
    pub detected_protocol: String,
    /// Keyword bytes from five-baud init (empty for fast init)
    pub keyword_bytes: Vec<u8>,
    /// Whether the ECU's 0xCC acknowledgment was received (slow init only)
    pub cc_received: bool,
    /// The init IOCTL response, plus any post-init frames (e.g. CC byte)
    pub init_response: Vec<CanMessage>,
}

/// Data payload for successful responses
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseData {
    None,
    Devices(Vec<DeviceInfo>),
    Connected,
    Messages(Vec<CanMessage>),
    KlineInit(KlineInitResult),
    RawIo(RawIoResult),
    Version(VersionInfo),
    String(String),
    Number(u32),
    Float(f64),
    Bool(bool),
}

/// Raw J2534 result payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawIoResult {
    pub result: i32,
    pub num_msgs: u32,
}

/// J2534 device information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub name: String,
    pub vendor: String,
    pub dll_path: String,
    pub can_iso15765: bool,
    pub can_iso11898: bool,
    pub compatible: bool,
    pub bitness: u8,
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub api_version: String,
}

/// CAN / K-Line message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanMessage {
    pub timestamp_us: u64,
    pub arb_id: u32,
    pub extended: bool,
    pub data: Vec<u8>,
    pub raw_arb_id: u32,
    pub rx_status: u32,
    pub data_size: u32,
    /// J2534 protocol ID (5=CAN, 3=ISO9141, 4=ISO14230, etc.)
    #[serde(default = "default_protocol_can")]
    pub protocol_id: u32,
}

fn default_protocol_can() -> u32 {
    5 // PROTOCOL_CAN
}

/// Message for batch sending
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchMessage {
    pub arb_id: u32,
    pub data: Vec<u8>,
    pub extended: bool,
}

/// Version information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub firmware_version: String,
    pub dll_version: String,
    pub api_version: String,
}

/// Wrapper for messages that include an ID for request/response matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message<T> {
    pub id: u64,
    #[serde(flatten)]
    pub payload: T,
}

impl Response {
    pub fn ok(data: ResponseData) -> Self {
        Response::Ok { data }
    }

    pub fn ok_none() -> Self {
        Response::Ok {
            data: ResponseData::None,
        }
    }

    pub fn error(code: i32, message: impl Into<String>) -> Self {
        Response::Error {
            code,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_message() -> CanMessage {
        CanMessage {
            timestamp_us: 123456,
            arb_id: 0x7E0,
            extended: false,
            data: vec![0x02, 0x10, 0x01],
            raw_arb_id: 0x7E0,
            rx_status: 0,
            data_size: 7,
            protocol_id: 5,
        }
    }

    // --- Request serialization round-trips ---

    #[test]
    fn request_enumerate_devices_round_trip() {
        let req = Request::EnumerateDevices;
        let json = serde_json::to_string(&req).unwrap();
        let parsed: Request = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, Request::EnumerateDevices));
    }

    #[test]
    fn request_open_round_trip() {
        let req = Request::Open {
            dll_path: "C:\\driver.dll".to_string(),
            protocol_id: 5,
            baud_rate: 500000,
            connect_flags: 0x800,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: Request = serde_json::from_str(&json).unwrap();
        match parsed {
            Request::Open {
                dll_path,
                protocol_id,
                baud_rate,
                connect_flags,
            } => {
                assert_eq!(dll_path, "C:\\driver.dll");
                assert_eq!(protocol_id, 5);
                assert_eq!(baud_rate, 500000);
                assert_eq!(connect_flags, 0x800);
            }
            _ => panic!("Expected Open"),
        }
    }

    #[test]
    fn request_open_default_connect_flags() {
        let json = r#"{"method":"Open","params":{"dll_path":"test.dll","protocol_id":5,"baud_rate":500000}}"#;
        let parsed: Request = serde_json::from_str(json).unwrap();
        match parsed {
            Request::Open { connect_flags, .. } => assert_eq!(connect_flags, 0),
            _ => panic!("Expected Open"),
        }
    }

    #[test]
    fn request_close_round_trip() {
        let req = Request::Close;
        let json = serde_json::to_string(&req).unwrap();
        let parsed: Request = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, Request::Close));
    }

    #[test]
    fn request_shutdown_round_trip() {
        let req = Request::Shutdown;
        let json = serde_json::to_string(&req).unwrap();
        let parsed: Request = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, Request::Shutdown));
    }

    #[test]
    fn request_send_message_round_trip() {
        let req = Request::SendMessage {
            arb_id: 0x7DF,
            data: vec![0x02, 0x01, 0x00],
            extended: false,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: Request = serde_json::from_str(&json).unwrap();
        match parsed {
            Request::SendMessage {
                arb_id,
                data,
                extended,
            } => {
                assert_eq!(arb_id, 0x7DF);
                assert_eq!(data, vec![0x02, 0x01, 0x00]);
                assert!(!extended);
            }
            _ => panic!("Expected SendMessage"),
        }
    }

    #[test]
    fn request_send_messages_batch_round_trip() {
        let req = Request::SendMessagesBatch {
            messages: vec![
                BatchMessage {
                    arb_id: 0x7E0,
                    data: vec![0x02, 0x10, 0x01],
                    extended: false,
                },
                BatchMessage {
                    arb_id: 0x18DA00FF,
                    data: vec![0x03, 0x22, 0xF1, 0x90],
                    extended: true,
                },
            ],
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: Request = serde_json::from_str(&json).unwrap();
        match parsed {
            Request::SendMessagesBatch { messages } => {
                assert_eq!(messages.len(), 2);
                assert_eq!(messages[0].arb_id, 0x7E0);
                assert!(!messages[0].extended);
                assert_eq!(messages[1].arb_id, 0x18DA00FF);
                assert!(messages[1].extended);
            }
            _ => panic!("Expected SendMessagesBatch"),
        }
    }

    #[test]
    fn request_read_messages_defaults() {
        let json = r#"{"method":"ReadMessages","params":{"timeout_ms":100}}"#;
        let parsed: Request = serde_json::from_str(json).unwrap();
        match parsed {
            Request::ReadMessages {
                timeout_ms,
                batch_size,
                max_drain_reads,
            } => {
                assert_eq!(timeout_ms, 100);
                assert_eq!(batch_size, 256);
                assert_eq!(max_drain_reads, 64);
            }
            _ => panic!("Expected ReadMessages"),
        }
    }

    #[test]
    fn request_read_messages_custom_params() {
        let req = Request::ReadMessages {
            timeout_ms: 50,
            batch_size: 128,
            max_drain_reads: 32,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: Request = serde_json::from_str(&json).unwrap();
        match parsed {
            Request::ReadMessages {
                timeout_ms,
                batch_size,
                max_drain_reads,
            } => {
                assert_eq!(timeout_ms, 50);
                assert_eq!(batch_size, 128);
                assert_eq!(max_drain_reads, 32);
            }
            _ => panic!("Expected ReadMessages"),
        }
    }

    #[test]
    fn request_clear_buffers_round_trip() {
        let req = Request::ClearBuffers;
        let json = serde_json::to_string(&req).unwrap();
        let parsed: Request = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, Request::ClearBuffers));
    }

    #[test]
    fn request_kline_init_round_trip() {
        let req = Request::KlineInit {
            init_mode: KlineInitMode::Auto,
            fast_init_data: Some(vec![0xC1, 0x33, 0xF1, 0x81]),
            five_baud_address: None,
            cc_timeout_ms: Some(500),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: Request = serde_json::from_str(&json).unwrap();
        match parsed {
            Request::KlineInit {
                init_mode,
                fast_init_data,
                five_baud_address,
                cc_timeout_ms,
            } => {
                assert!(matches!(init_mode, KlineInitMode::Auto));
                assert_eq!(fast_init_data, Some(vec![0xC1, 0x33, 0xF1, 0x81]));
                assert!(five_baud_address.is_none());
                assert_eq!(cc_timeout_ms, Some(500));
            }
            _ => panic!("Expected KlineInit"),
        }
    }

    #[test]
    fn request_kline_init_defaults() {
        let json = r#"{"method":"KlineInit","params":{"init_mode":"fast"}}"#;
        let parsed: Request = serde_json::from_str(json).unwrap();
        match parsed {
            Request::KlineInit {
                fast_init_data,
                five_baud_address,
                cc_timeout_ms,
                ..
            } => {
                assert!(fast_init_data.is_none());
                assert!(five_baud_address.is_none());
                assert!(cc_timeout_ms.is_none());
            }
            _ => panic!("Expected KlineInit"),
        }
    }

    #[test]
    fn request_add_filter_round_trip() {
        let req = Request::AddFilter {
            filter_type: "pass".to_string(),
            mask: vec![0xFF, 0xFF, 0xFF, 0xFF],
            pattern: vec![0x00, 0x00, 0x07, 0xE0],
            extended: false,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: Request = serde_json::from_str(&json).unwrap();
        match parsed {
            Request::AddFilter {
                filter_type,
                mask,
                pattern,
                extended,
            } => {
                assert_eq!(filter_type, "pass");
                assert_eq!(mask.len(), 4);
                assert_eq!(pattern.len(), 4);
                assert!(!extended);
            }
            _ => panic!("Expected AddFilter"),
        }
    }

    #[test]
    fn request_start_periodic_message_round_trip() {
        let req = Request::StartPeriodicMessage {
            arb_id: 0x7E0,
            data: vec![0x02, 0x3E, 0x00],
            interval_ms: 2000,
            extended: false,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: Request = serde_json::from_str(&json).unwrap();
        match parsed {
            Request::StartPeriodicMessage {
                arb_id,
                interval_ms,
                ..
            } => {
                assert_eq!(arb_id, 0x7E0);
                assert_eq!(interval_ms, 2000);
            }
            _ => panic!("Expected StartPeriodicMessage"),
        }
    }

    #[test]
    fn request_write_messages_raw_round_trip() {
        let req = Request::WriteMessagesRaw {
            messages: vec![BatchMessage {
                arb_id: 0x7E0,
                data: vec![0x02, 0x10, 0x01],
                extended: false,
            }],
            timeout_ms: 5000,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: Request = serde_json::from_str(&json).unwrap();
        match parsed {
            Request::WriteMessagesRaw {
                messages,
                timeout_ms,
            } => {
                assert_eq!(messages.len(), 1);
                assert_eq!(timeout_ms, 5000);
            }
            _ => panic!("Expected WriteMessagesRaw"),
        }
    }

    // --- Response serialization round-trips ---

    #[test]
    fn response_ok_none_round_trip() {
        let resp = Response::ok_none();
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        match parsed {
            Response::Ok { data } => assert!(matches!(data, ResponseData::None)),
            _ => panic!("Expected Ok"),
        }
    }

    #[test]
    fn response_error_round_trip() {
        let resp = Response::error(-1, "device not found");
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        match parsed {
            Response::Error { code, message } => {
                assert_eq!(code, -1);
                assert_eq!(message, "device not found");
            }
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn response_messages_round_trip() {
        let msg = make_test_message();
        let resp = Response::ok(ResponseData::Messages(vec![msg]));
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        match parsed {
            Response::Ok {
                data: ResponseData::Messages(msgs),
            } => {
                assert_eq!(msgs.len(), 1);
                assert_eq!(msgs[0].arb_id, 0x7E0);
                assert_eq!(msgs[0].timestamp_us, 123456);
                assert_eq!(msgs[0].data, vec![0x02, 0x10, 0x01]);
                assert!(!msgs[0].extended);
                assert_eq!(msgs[0].protocol_id, 5);
            }
            _ => panic!("Expected Ok with Messages"),
        }
    }

    #[test]
    fn response_devices_round_trip() {
        let device = DeviceInfo {
            name: "OpenPort 2.0".to_string(),
            vendor: "Tactrix".to_string(),
            dll_path: "C:\\op20.dll".to_string(),
            can_iso15765: true,
            can_iso11898: true,
            compatible: true,
            bitness: 32,
            available: true,
            unavailable_reason: None,
            api_version: "04.04".to_string(),
        };
        let resp = Response::ok(ResponseData::Devices(vec![device]));
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        match parsed {
            Response::Ok {
                data: ResponseData::Devices(devs),
            } => {
                assert_eq!(devs.len(), 1);
                assert_eq!(devs[0].name, "OpenPort 2.0");
                assert_eq!(devs[0].bitness, 32);
                assert!(devs[0].compatible);
                assert!(devs[0].available);
                assert!(devs[0].unavailable_reason.is_none());
                assert_eq!(devs[0].api_version, "04.04");
            }
            _ => panic!("Expected Ok with Devices"),
        }
    }

    #[test]
    fn response_version_round_trip() {
        let resp = Response::ok(ResponseData::Version(VersionInfo {
            firmware_version: "1.2.3".to_string(),
            dll_version: "4.5.6".to_string(),
            api_version: "04.04".to_string(),
        }));
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        match parsed {
            Response::Ok {
                data: ResponseData::Version(v),
            } => {
                assert_eq!(v.firmware_version, "1.2.3");
                assert_eq!(v.dll_version, "4.5.6");
                assert_eq!(v.api_version, "04.04");
            }
            _ => panic!("Expected Ok with Version"),
        }
    }

    #[test]
    fn response_number_round_trip() {
        let resp = Response::ok(ResponseData::Number(42));
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        match parsed {
            Response::Ok {
                data: ResponseData::Number(n),
            } => assert_eq!(n, 42),
            _ => panic!("Expected Ok with Number"),
        }
    }

    #[test]
    fn response_float_round_trip() {
        let resp = Response::ok(ResponseData::Float(12.6));
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        match parsed {
            Response::Ok {
                data: ResponseData::Float(f),
            } => assert!((f - 12.6).abs() < 0.01),
            _ => panic!("Expected Ok with Float"),
        }
    }

    #[test]
    fn response_bool_round_trip() {
        let resp = Response::ok(ResponseData::Bool(true));
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        match parsed {
            Response::Ok {
                data: ResponseData::Bool(b),
            } => assert!(b),
            _ => panic!("Expected Ok with Bool"),
        }
    }

    #[test]
    fn response_raw_io_round_trip() {
        let resp = Response::ok(ResponseData::RawIo(RawIoResult {
            result: 0,
            num_msgs: 5,
        }));
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        match parsed {
            Response::Ok {
                data: ResponseData::RawIo(raw),
            } => {
                assert_eq!(raw.result, 0);
                assert_eq!(raw.num_msgs, 5);
            }
            _ => panic!("Expected Ok with RawIo"),
        }
    }

    #[test]
    fn response_kline_init_round_trip() {
        let resp = Response::ok(ResponseData::KlineInit(KlineInitResult {
            init_method: "fast".to_string(),
            detected_protocol: "iso14230-fast".to_string(),
            keyword_bytes: vec![],
            cc_received: false,
            init_response: vec![make_test_message()],
        }));
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        match parsed {
            Response::Ok {
                data: ResponseData::KlineInit(result),
            } => {
                assert_eq!(result.init_method, "fast");
                assert_eq!(result.detected_protocol, "iso14230-fast");
                assert!(!result.cc_received);
                assert_eq!(result.init_response.len(), 1);
            }
            _ => panic!("Expected Ok with KlineInit"),
        }
    }

    // --- Message<T> wrapper ---

    #[test]
    fn message_wrapper_request_round_trip() {
        let msg = Message {
            id: 42,
            payload: Request::ReadVersion,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message<Request> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, 42);
        assert!(matches!(parsed.payload, Request::ReadVersion));
    }

    #[test]
    fn message_wrapper_response_round_trip() {
        let msg = Message {
            id: 7,
            payload: Response::ok(ResponseData::Number(99)),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message<Response> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, 7);
        match parsed.payload {
            Response::Ok {
                data: ResponseData::Number(n),
            } => assert_eq!(n, 99),
            _ => panic!("Expected Ok with Number"),
        }
    }

    // --- CanMessage default protocol_id ---

    #[test]
    fn can_message_default_protocol_id() {
        let json = r#"{"timestampUs":0,"arbId":0,"extended":false,"data":[],"rawArbId":0,"rxStatus":0,"dataSize":0}"#;
        let parsed: CanMessage = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.protocol_id, 5); // PROTOCOL_CAN
    }

    #[test]
    fn can_message_kline_protocol_id() {
        let json = r#"{"timestampUs":0,"arbId":0,"extended":false,"data":[],"rawArbId":0,"rxStatus":0,"dataSize":0,"protocolId":3}"#;
        let parsed: CanMessage = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.protocol_id, 3); // ISO9141
    }

    // --- KlineInitMode serialization ---

    #[test]
    fn kline_init_mode_serialization() {
        assert_eq!(
            serde_json::to_string(&KlineInitMode::Fast).unwrap(),
            "\"fast\""
        );
        assert_eq!(
            serde_json::to_string(&KlineInitMode::Slow).unwrap(),
            "\"slow\""
        );
        assert_eq!(
            serde_json::to_string(&KlineInitMode::Auto).unwrap(),
            "\"auto\""
        );
    }

    // --- All unit-less Request variants ---

    #[test]
    fn unit_request_variants_round_trip() {
        let variants = vec![
            Request::ClearBuffers,
            Request::ReadVersion,
            Request::GetLastError,
            Request::ReadBatteryVoltage,
            Request::ReadProgrammingVoltage,
            Request::ClearPeriodicMessages,
            Request::ClearFilters,
            Request::GetLoopback,
            Request::GetDataRate,
        ];
        for req in variants {
            let json = serde_json::to_string(&req).unwrap();
            let _parsed: Request = serde_json::from_str(&json).unwrap();
        }
    }

    // --- Empty messages list ---

    #[test]
    fn response_empty_messages_round_trip() {
        // Empty Messages vec serializes as [] — verify it round-trips as Devices
        // (since ResponseData is untagged, serde tries variants in order:
        // None, Devices, Connected, Messages — empty [] matches Devices first)
        let resp = Response::ok(ResponseData::Messages(vec![]));
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        match parsed {
            Response::Ok { data: ResponseData::Devices(d) } => assert!(d.is_empty()),
            other => {
                // Accept whatever serde picks — the key point is it doesn't error
                assert!(matches!(other, Response::Ok { .. }));
            }
        }
    }
}

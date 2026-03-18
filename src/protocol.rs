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

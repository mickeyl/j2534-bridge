//! Bridge Client
//!
//! Communicates with the j2534-bridge process via named pipes.

use crate::protocol::{
    BatchMessage, CanMessage, DeviceInfo, Message, RawIoResult, Request, Response, ResponseData,
    VersionInfo,
};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(windows)]
use std::os::windows::io::FromRawHandle;

/// Bridge client that manages the bridge process and communication
pub struct BridgeClient {
    process: Option<Child>,
    pipe_name: String,
    writer: Option<std::fs::File>,
    reader: Option<BufReader<std::fs::File>>,
    next_id: AtomicU64,
}

impl BridgeClient {
    /// Create a new bridge client (doesn't start the bridge yet)
    pub fn new() -> Self {
        use std::sync::atomic::AtomicU64 as Counter;
        static INSTANCE: Counter = Counter::new(0);
        let instance = INSTANCE.fetch_add(1, Ordering::SeqCst);
        let pipe_name = format!(
            "\\\\.\\pipe\\j2534-bridge-{}-{}",
            std::process::id(),
            instance
        );

        Self {
            process: None,
            pipe_name,
            writer: None,
            reader: None,
            next_id: AtomicU64::new(1),
        }
    }

    /// Get the PID of the bridge process (if running)
    pub fn pid(&self) -> Option<u32> {
        self.process.as_ref().map(|c| c.id())
    }

    /// Get the path to the bridge executable for the given bitness
    pub fn get_bridge_path(bitness: u8) -> Result<std::path::PathBuf, String> {
        let exe_dir = std::env::current_exe()
            .map_err(|e| format!("Failed to get executable path: {}", e))?
            .parent()
            .ok_or("Failed to get executable directory")?
            .to_path_buf();
        let process_bits = (std::mem::size_of::<usize>() * 8) as u8;

        let bridge_name = if bitness == 32 {
            "j2534-bridge-32.exe"
        } else {
            "j2534-bridge-64.exe"
        };

        // Try production location (same directory as main exe)
        let bridge_path = exe_dir.join(bridge_name);
        if bridge_path.exists() {
            return Ok(bridge_path);
        }

        // Try Tauri resource directory (NSIS installs place resources here)
        let resource_path = exe_dir.join("resources").join(bridge_name);
        if resource_path.exists() {
            return Ok(resource_path);
        }

        // In development, prefer the explicitly published bridge binaries.
        // This avoids accidentally launching an older cross-target artifact from target/.
        let dev_published_candidates = [
            exe_dir
                .parent() // target/<triple>/
                .and_then(|p| p.parent()) // target/
                .and_then(|p| p.parent()) // src-tauri/
                .map(|p| p.join("bin").join(bridge_name)),
            exe_dir
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
                .and_then(|p| p.parent()) // CANcorder/
                .map(|p| p.join("src-tauri").join("bin").join(bridge_name)),
        ];

        for candidate in dev_published_candidates.iter().flatten() {
            eprintln!(
                "[j2534-client] Looking for published dev bridge at: {:?}",
                candidate
            );
            if candidate.exists() {
                return Ok(candidate.clone());
            }
        }

        // Try in the j2534-bridge target directory (for development)
        let target_triple = if bitness == 32 {
            "i686-pc-windows-msvc"
        } else {
            "x86_64-pc-windows-msvc"
        };

        // Try debug build first so local development picks up the freshly built bridge.
        for build_type in &["debug", "release"] {
            // Support three development layouts:
            // 1. Running inside j2534-bridge itself: target/debug/j2534-dump.exe
            // 2. From a sibling app crate in the same workspace
            // 3. From a separate repo pointing at ../j2534-bridge
            let candidates = [
                // Local crate native build: only valid when requested bitness matches this process.
                exe_dir.parent().and_then(|p| {
                    if process_bits == bitness {
                        Some(p.join(build_type).join("j2534-bridge.exe"))
                    } else {
                        None
                    }
                }),
                // Local crate cross-target build: j2534-bridge/target/<triple>/debug/j2534-bridge.exe
                exe_dir.parent().map(|p| {
                    p.join(target_triple)
                        .join(build_type)
                        .join("j2534-bridge.exe")
                }),
                // Workspace sibling: CANcorder/j2534-bridge/target/...
                exe_dir
                    .parent() // target/<triple>/
                    .and_then(|p| p.parent()) // target/
                    .and_then(|p| p.parent()) // src-tauri/
                    .and_then(|p| p.parent()) // CANcorder/
                    .map(|p| {
                        p.join("j2534-bridge")
                            .join("target")
                            .join(target_triple)
                            .join(build_type)
                            .join("j2534-bridge.exe")
                    }),
                // Git submodule at src-tauri/j2534-bridge/ (debug: target/debug/,
                // cross-compiled: target/<triple>/debug/).  Walk up to the directory
                // that *contains* `target/` and look for j2534-bridge/ there.
                {
                    let target_dir = exe_dir.parent(); // .../target
                    let crate_root = target_dir.and_then(|p| p.parent()); // .../src-tauri
                    crate_root.map(|p| {
                        p.join("j2534-bridge")
                            .join("target")
                            .join(target_triple)
                            .join(build_type)
                            .join("j2534-bridge.exe")
                    })
                },
                // Shared crate: ../j2534-bridge/target/...
                exe_dir
                    .parent()
                    .and_then(|p| p.parent())
                    .and_then(|p| p.parent())
                    .and_then(|p| p.parent())
                    .and_then(|p| p.parent()) // up to Documents/late/
                    .map(|p| {
                        p.join("j2534-bridge")
                            .join("target")
                            .join(target_triple)
                            .join(build_type)
                            .join("j2534-bridge.exe")
                    }),
            ];

            for candidate in candidates.iter().flatten() {
                eprintln!("[j2534-client] Looking for bridge at: {:?}", candidate);
                if candidate.exists() {
                    return Ok(candidate.clone());
                }
            }
        }

        Err(format!(
            "Bridge executable not found: {} (looked in {:?}, {:?}/resources, and development paths)",
            bridge_name, exe_dir, exe_dir
        ))
    }

    /// Start the bridge process for the given DLL bitness
    #[cfg(windows)]
    pub fn start(&mut self, bitness: u8) -> Result<(), String> {
        use windows::core::PCSTR;
        use windows::Win32::Storage::FileSystem::{
            CreateFileA, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_NONE, OPEN_EXISTING,
        };

        if self.process.is_some() {
            return Err("Bridge already running".into());
        }

        let bridge_path = Self::get_bridge_path(bitness)?;
        eprintln!(
            "[J2534_DEBUG] j2534-client starting bridge bitness={} path={:?}",
            bitness, bridge_path
        );
        eprintln!("[J2534_DEBUG] j2534-client pipe={}", self.pipe_name);

        // Start the bridge process (hidden — no console window)
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let child = Command::new(&bridge_path)
            .arg(&self.pipe_name)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to start bridge: {}", e))?;

        self.process = Some(child);

        // Give the bridge time to create the pipe
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Connect to the named pipe
        let pipe_name_cstr = std::ffi::CString::new(self.pipe_name.as_str())
            .map_err(|e| format!("Invalid pipe name: {}", e))?;

        let pipe_handle = unsafe {
            CreateFileA(
                PCSTR::from_raw(pipe_name_cstr.as_ptr() as *const u8),
                (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }
        .map_err(|e| format!("Failed to connect to pipe: {}", e))?;

        // Convert to std::fs::File
        let handle_raw = pipe_handle.0 as *mut std::ffi::c_void;
        let file = unsafe { std::fs::File::from_raw_handle(handle_raw) };

        let reader_file = file
            .try_clone()
            .map_err(|e| format!("Failed to clone pipe handle: {}", e))?;

        self.writer = Some(file);
        self.reader = Some(BufReader::new(reader_file));

        eprintln!("[j2534-client] Connected to bridge");
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn start(&mut self, _bitness: u8) -> Result<(), String> {
        Err("Bridge client is only supported on Windows".into())
    }

    /// Stop the bridge process
    pub fn stop(&mut self) -> Result<(), String> {
        if self.writer.is_some() {
            let _ = self.send_request(Request::Shutdown);
        }

        self.writer = None;
        self.reader = None;

        if let Some(mut child) = self.process.take() {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = child.kill();
            let _ = child.wait();
        }

        Ok(())
    }

    /// Send a request and wait for the response
    pub fn send_request(&mut self, request: Request) -> Result<Response, String> {
        let request_debug = format!("{:?}", request);

        let writer = self.writer.as_mut().ok_or("Bridge not connected")?;
        let reader = self.reader.as_mut().ok_or("Bridge not connected")?;

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let msg = Message {
            id,
            payload: request,
        };

        let json = serde_json::to_string(&msg)
            .map_err(|e| format!("Failed to serialize request: {}", e))?;

        writeln!(writer, "{}", json).map_err(|e| format!("Failed to write to pipe: {}", e))?;
        writer
            .flush()
            .map_err(|e| format!("Failed to flush pipe: {}", e))?;

        // Read response
        let mut line = String::new();
        let bytes_read = reader
            .read_line(&mut line)
            .map_err(|e| format!("Failed to read from pipe: {}", e))?;

        if bytes_read == 0 || line.trim().is_empty() {
            let bridge_status = self.check_bridge_status();
            return Err(format!(
                "Bridge process died while handling request: {}. {}",
                request_debug, bridge_status
            ));
        }

        let response: Message<Response> = serde_json::from_str(&line).map_err(|e| {
            format!(
                "Failed to parse bridge response: {}. Raw: {:?}",
                e,
                if line.len() > 200 {
                    &line[..200]
                } else {
                    &line
                }
            )
        })?;

        if response.id != id {
            return Err(format!(
                "Response ID mismatch: expected {}, got {}",
                id, response.id
            ));
        }

        Ok(response.payload)
    }

    /// Check if the bridge process is still running
    fn check_bridge_status(&mut self) -> String {
        if let Some(ref mut child) = self.process {
            match child.try_wait() {
                Ok(Some(status)) => {
                    format!(
                        "Bridge exited with status: {}. The J2534 DLL may have crashed.",
                        status
                    )
                }
                Ok(None) => {
                    "Bridge process still running but pipe closed unexpectedly.".to_string()
                }
                Err(e) => format!("Could not check bridge status: {}", e),
            }
        } else {
            "Bridge process handle not available.".to_string()
        }
    }

    /// Check if the bridge is running
    pub fn is_running(&self) -> bool {
        self.process.is_some() && self.writer.is_some()
    }

    // ---- Convenience methods that wrap send_request ----

    /// Enumerate J2534 devices visible to the bridge process
    pub fn enumerate_devices(&mut self) -> Result<Vec<DeviceInfo>, String> {
        let response = self.send_request(Request::EnumerateDevices)?;
        match response {
            Response::Ok {
                data: ResponseData::Devices(devices),
            } => Ok(devices),
            Response::Ok { .. } => Ok(Vec::new()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Open a J2534 connection
    /// Open a J2534 connection.
    /// connect_flags: raw J2534 flags (0x800 = CAN_ID_BOTH, 0x100 = CAN_29BIT_ID, 0 = 11-bit only)
    pub fn open(
        &mut self,
        dll_path: &str,
        protocol_id: u32,
        baud_rate: u32,
        connect_flags: u32,
    ) -> Result<(), String> {
        let response = self.send_request(Request::Open {
            dll_path: dll_path.to_string(),
            protocol_id,
            baud_rate,
            connect_flags,
        })?;
        match response {
            Response::Ok { .. } => Ok(()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Close the J2534 connection
    pub fn close_connection(&mut self) -> Result<(), String> {
        let response = self.send_request(Request::Close)?;
        match response {
            Response::Ok { .. } => Ok(()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Read messages with drain loop (high-throughput)
    pub fn read_messages_drain(
        &mut self,
        timeout_ms: u32,
        batch_size: u32,
        max_drain_reads: u32,
    ) -> Result<Vec<CanMessage>, String> {
        let response = self.send_request(Request::ReadMessages {
            timeout_ms,
            batch_size,
            max_drain_reads,
        })?;
        match response {
            Response::Ok {
                data: ResponseData::Messages(msgs),
            } => Ok(msgs),
            Response::Ok { .. } => Ok(Vec::new()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Read messages (simple, low-throughput)
    pub fn read_messages(&mut self, timeout_ms: u32) -> Result<Vec<CanMessage>, String> {
        self.read_messages_drain(timeout_ms, 16, 1)
    }

    /// Read messages including loopback echoes
    pub fn read_messages_with_loopback(
        &mut self,
        timeout_ms: u32,
    ) -> Result<Vec<CanMessage>, String> {
        let response = self.send_request(Request::ReadMessagesWithLoopback { timeout_ms })?;
        match response {
            Response::Ok {
                data: ResponseData::Messages(msgs),
            } => Ok(msgs),
            Response::Ok { .. } => Ok(Vec::new()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Read messages with raw result
    pub fn read_messages_raw(
        &mut self,
        timeout_ms: u32,
        max_msgs: u32,
    ) -> Result<RawIoResult, String> {
        let response = self.send_request(Request::ReadMessagesRaw {
            timeout_ms,
            max_msgs,
        })?;
        match response {
            Response::Ok {
                data: ResponseData::RawIo(raw),
            } => Ok(raw),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Send a single CAN message
    pub fn send_message(&mut self, arb_id: u32, data: &[u8], extended: bool) -> Result<(), String> {
        let response = self.send_request(Request::SendMessage {
            arb_id,
            data: data.to_vec(),
            extended,
        })?;
        match response {
            Response::Ok { .. } => Ok(()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Send multiple CAN messages in a batch
    pub fn send_messages_batch(
        &mut self,
        messages: Vec<(u32, Vec<u8>, bool)>,
    ) -> Result<u32, String> {
        let batch: Vec<BatchMessage> = messages
            .into_iter()
            .map(|(arb_id, data, extended)| BatchMessage {
                arb_id,
                data,
                extended,
            })
            .collect();

        let response = self.send_request(Request::SendMessagesBatch { messages: batch })?;
        match response {
            Response::Ok {
                data: ResponseData::Number(n),
            } => Ok(n),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Write messages with custom timeout (raw)
    pub fn write_messages_raw(
        &mut self,
        messages: Vec<(u32, Vec<u8>, bool)>,
        timeout_ms: u32,
    ) -> Result<RawIoResult, String> {
        let batch: Vec<BatchMessage> = messages
            .into_iter()
            .map(|(arb_id, data, extended)| BatchMessage {
                arb_id,
                data,
                extended,
            })
            .collect();

        let response = self.send_request(Request::WriteMessagesRaw {
            messages: batch,
            timeout_ms,
        })?;
        match response {
            Response::Ok {
                data: ResponseData::RawIo(raw),
            } => Ok(raw),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Clear TX and RX buffers
    pub fn clear_buffers(&mut self) -> Result<(), String> {
        let response = self.send_request(Request::ClearBuffers)?;
        match response {
            Response::Ok { .. } => Ok(()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Read version info
    pub fn read_version(&mut self) -> Result<VersionInfo, String> {
        let response = self.send_request(Request::ReadVersion)?;
        match response {
            Response::Ok {
                data: ResponseData::Version(v),
            } => Ok(v),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Get last error string
    pub fn get_last_error_string(&mut self) -> Result<String, String> {
        let response = self.send_request(Request::GetLastError)?;
        match response {
            Response::Ok {
                data: ResponseData::String(s),
            } => Ok(s),
            Response::Ok { .. } => Ok(String::new()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Read battery voltage
    pub fn read_battery_voltage(&mut self) -> Result<f64, String> {
        let response = self.send_request(Request::ReadBatteryVoltage)?;
        match response {
            Response::Ok {
                data: ResponseData::Float(v),
            } => Ok(v),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Read programming voltage
    pub fn read_programming_voltage(&mut self) -> Result<f64, String> {
        let response = self.send_request(Request::ReadProgrammingVoltage)?;
        match response {
            Response::Ok {
                data: ResponseData::Float(v),
            } => Ok(v),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    pub fn fast_init(&mut self, data: &[u8]) -> Result<CanMessage, String> {
        let response = self.send_request(Request::FastInit {
            data: data.to_vec(),
        })?;
        match response {
            Response::Ok {
                data: ResponseData::Messages(mut msgs),
            } => msgs
                .drain(..)
                .next()
                .ok_or_else(|| "No FAST_INIT response message returned".to_string()),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    pub fn five_baud_init(&mut self, data: &[u8]) -> Result<CanMessage, String> {
        let response = self.send_request(Request::FiveBaudInit {
            data: data.to_vec(),
        })?;
        match response {
            Response::Ok {
                data: ResponseData::Messages(mut msgs),
            } => msgs
                .drain(..)
                .next()
                .ok_or_else(|| "No FIVE_BAUD_INIT response message returned".to_string()),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Full K-Line init (fast/slow/auto) with CC polling — runs inside bridge.
    pub fn kline_init(
        &mut self,
        init_mode: crate::protocol::KlineInitMode,
        fast_init_data: Option<Vec<u8>>,
        five_baud_address: Option<Vec<u8>>,
        cc_timeout_ms: Option<u32>,
    ) -> Result<crate::protocol::KlineInitResult, String> {
        let response = self.send_request(Request::KlineInit {
            init_mode,
            fast_init_data,
            five_baud_address,
            cc_timeout_ms,
        })?;
        match response {
            Response::Ok {
                data: ResponseData::KlineInit(r),
            } => Ok(r),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Start a periodic message
    pub fn start_periodic_message(
        &mut self,
        arb_id: u32,
        data: &[u8],
        interval_ms: u32,
        extended: bool,
    ) -> Result<u32, String> {
        let response = self.send_request(Request::StartPeriodicMessage {
            arb_id,
            data: data.to_vec(),
            interval_ms,
            extended,
        })?;
        match response {
            Response::Ok {
                data: ResponseData::Number(id),
            } => Ok(id),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Stop a periodic message
    pub fn stop_periodic_message(&mut self, msg_id: u32) -> Result<(), String> {
        let response = self.send_request(Request::StopPeriodicMessage { msg_id })?;
        match response {
            Response::Ok { .. } => Ok(()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Clear all periodic messages
    pub fn clear_periodic_messages(&mut self) -> Result<(), String> {
        let response = self.send_request(Request::ClearPeriodicMessages)?;
        match response {
            Response::Ok { .. } => Ok(()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Add a message filter
    pub fn add_filter(
        &mut self,
        filter_type: u32,
        mask: &[u8],
        pattern: &[u8],
        extended: bool,
    ) -> Result<u32, String> {
        let ft = match filter_type {
            1 => "pass",
            2 => "block",
            3 => "flow_control",
            _ => return Err("Invalid filter type".to_string()),
        };
        let response = self.send_request(Request::AddFilter {
            filter_type: ft.to_string(),
            mask: mask.to_vec(),
            pattern: pattern.to_vec(),
            extended,
        })?;
        match response {
            Response::Ok {
                data: ResponseData::Number(id),
            } => Ok(id),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Add a raw filter
    pub fn add_filter_raw(
        &mut self,
        filter_type: u32,
        mask: &[u8],
        pattern: &[u8],
        extended: bool,
    ) -> Result<u32, String> {
        let ft = match filter_type {
            1 => "pass",
            2 => "block",
            3 => "flow_control",
            _ => return Err("Invalid filter type".to_string()),
        };
        let response = self.send_request(Request::AddFilterRaw {
            filter_type: ft.to_string(),
            mask: mask.to_vec(),
            pattern: pattern.to_vec(),
            extended,
        })?;
        match response {
            Response::Ok {
                data: ResponseData::Number(id),
            } => Ok(id),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Remove a message filter
    pub fn remove_filter(&mut self, filter_id: u32) -> Result<(), String> {
        let response = self.send_request(Request::RemoveFilter { filter_id })?;
        match response {
            Response::Ok { .. } => Ok(()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Clear all filters
    pub fn clear_filters(&mut self) -> Result<(), String> {
        let response = self.send_request(Request::ClearFilters)?;
        match response {
            Response::Ok { .. } => Ok(()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Get a configuration parameter
    pub fn get_config(&mut self, parameter: u32) -> Result<u32, String> {
        let response = self.send_request(Request::GetConfig { parameter })?;
        match response {
            Response::Ok {
                data: ResponseData::Number(v),
            } => Ok(v),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Set a configuration parameter
    pub fn set_config(&mut self, parameter: u32, value: u32) -> Result<(), String> {
        let response = self.send_request(Request::SetConfig { parameter, value })?;
        match response {
            Response::Ok { .. } => Ok(()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Get loopback setting
    pub fn get_loopback(&mut self) -> Result<bool, String> {
        let response = self.send_request(Request::GetLoopback)?;
        match response {
            Response::Ok {
                data: ResponseData::Bool(v),
            } => Ok(v),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Set loopback setting
    pub fn set_loopback(&mut self, enabled: bool) -> Result<(), String> {
        let response = self.send_request(Request::SetLoopback { enabled })?;
        match response {
            Response::Ok { .. } => Ok(()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Get current data rate
    pub fn get_data_rate(&mut self) -> Result<u32, String> {
        let response = self.send_request(Request::GetDataRate)?;
        match response {
            Response::Ok {
                data: ResponseData::Number(v),
            } => Ok(v),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }
}

impl Drop for BridgeClient {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl Default for BridgeClient {
    fn default() -> Self {
        Self::new()
    }
}

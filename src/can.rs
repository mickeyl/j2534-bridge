//! Raw CAN contract for the SAE J2534-2 CAN-FD extension.
//! The GMW17753 CAN_FD_PS dialect has different identifiers and is not implied.
use serde::{Deserialize, Serialize};

pub const FD_CAN_PS: u32 = 0x8011;
pub const FD_CAN_DATA_PHASE_RATE: u32 = 0x805c;
pub const J1962_PINS: u32 = 0x8001;
pub const FD_CAN_BRS: u32 = 1 << 19;
pub const FD_CAN_FORMAT: u32 = 1 << 20;
pub const FD_CAN_ESI: u32 = 1 << 21;

/// Registry hints for the implemented SAE raw-FD dialect. PEAK also implements
/// SAE FD in installations which advertise only the older GMW registry key.
/// Its 05.00 registry entries omit per-protocol keys altogether.
/// Opening still checks driver/device acceptance and reads back the FD data rate.
pub fn supports_raw_can_fd(vendor: &str, protocols: &[String], api_version: &str) -> bool {
    protocols.iter().any(|p| p == "CAN-FD")
        || (vendor.to_ascii_lowercase().contains("peak")
            && (api_version == "05.00" || protocols.iter().any(|p| p == "CAN-FD (GMW)")))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanOpenOptions {
    pub nominal_bitrate: u32,
    /// Some enables SAE raw CAN FD; None selects Classic CAN.
    pub data_bitrate: Option<u32>,
    /// Vendor-defined PassThruOpen name, e.g. J2534-2:PEAK 0x51.
    pub device_name: Option<String>,
    #[serde(default)]
    pub listen_only: bool,
}

impl CanOpenOptions {
    pub fn validate(&self) -> Result<(), String> {
        if self.listen_only {
            return Err("ERR_J2534_LISTEN_ONLY_UNSUPPORTED: loopback control does not silence the CAN controller".into());
        }
        self.validate_rates()
    }

    pub fn validate_rates(&self) -> Result<(), String> {
        if self.nominal_bitrate == 0 || self.data_bitrate.is_some_and(|r| r < self.nominal_bitrate)
        {
            return Err("ERR_J2534_BITRATE: nominal bitrate must be positive and data bitrate must be at least nominal bitrate".into());
        }
        if self.device_name.as_ref().is_some_and(|n| n.contains('\0')) {
            return Err("ERR_J2534_DEVICE_NAME: embedded NUL".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanTxFrame {
    pub arb_id: u32,
    pub data: Vec<u8>,
    #[serde(default)]
    pub extended: bool,
    #[serde(default)]
    pub is_fd: bool,
    #[serde(default)]
    pub bitrate_switch: bool,
}

impl CanTxFrame {
    pub fn encode(&self, fd_channel: bool) -> Result<(u32, Vec<u8>), String> {
        if self.arb_id > if self.extended { 0x1fff_ffff } else { 0x7ff } {
            return Err("ERR_J2534_CAN_ID: arbitration ID exceeds selected width".into());
        }
        if (self.is_fd && !fd_channel) || (self.bitrate_switch && !self.is_fd) {
            return Err(
                "ERR_J2534_CAN_FLAGS: FD requires an FD channel; BRS requires an FD frame".into(),
            );
        }
        let len = self.data.len();
        if if self.is_fd {
            !matches!(len, 0..=8 | 12 | 16 | 20 | 24 | 32 | 48 | 64)
        } else {
            len > 8
        } {
            return Err("ERR_J2534_CAN_LENGTH: use an exact CAN wire length (Classic 0..8; FD also 12,16,20,24,32,48,64)".into());
        }
        let flags = if self.extended { 0x100 } else { 0 }
            | if self.is_fd { FD_CAN_FORMAT } else { 0 }
            | if self.bitrate_switch { FD_CAN_BRS } else { 0 };
        let mut bytes = self.arb_id.to_be_bytes().to_vec();
        bytes.extend_from_slice(&self.data);
        Ok((flags, bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_fd_capability_does_not_infer_sae_from_iso_tp_or_other_vendor_gmw() {
        assert!(supports_raw_can_fd("PEAK-System", &[], "05.00"));
        assert!(!supports_raw_can_fd("Other", &[], "05.00"));
        assert!(supports_raw_can_fd("Other", &["CAN-FD".into()], "04.04"));
        assert!(supports_raw_can_fd(
            "PEAK-System",
            &["CAN-FD (GMW)".into()],
            "04.04"
        ));
        assert!(!supports_raw_can_fd(
            "Other",
            &["CAN-FD (GMW)".into()],
            "04.04"
        ));
        assert!(!supports_raw_can_fd(
            "Other",
            &["ISO-TP FD".into()],
            "05.00"
        ));
    }
    #[test]
    fn wire_lengths_flags_and_identifier_widths() {
        for len in 0..=65 {
            let frame = CanTxFrame {
                arb_id: 0x18da00f1,
                data: vec![0xa5; len],
                extended: true,
                is_fd: true,
                bitrate_switch: true,
            };
            let encoded = frame.encode(true);
            assert_eq!(
                encoded.is_ok(),
                matches!(len, 0..=8 | 12 | 16 | 20 | 24 | 32 | 48 | 64)
            );
            if let Ok((flags, bytes)) = encoded {
                assert_eq!(flags, 0x180100);
                assert_eq!(&bytes[..4], &[0x18, 0xda, 0, 0xf1]);
                assert_eq!(&bytes[4..], frame.data);
            }
            assert!(frame.encode(false).is_err());
        }
    }
    #[test]
    fn rejects_inconsistent_classic_frames() {
        let mut frame = CanTxFrame {
            arb_id: 0x123,
            data: vec![0; 8],
            extended: false,
            is_fd: false,
            bitrate_switch: false,
        };
        assert_eq!(frame.encode(true).unwrap().0, 0);
        frame.bitrate_switch = true;
        assert!(frame.encode(true).is_err());
        frame.bitrate_switch = false;
        frame.data.push(0);
        assert!(frame.encode(false).is_err());
        frame.data.clear();
        frame.arb_id = 0x800;
        assert!(frame.encode(true).is_err());
        frame.extended = true;
        assert!(frame.encode(true).is_ok());
    }
    #[test]
    fn rejects_false_silent_mode_and_invalid_rates() {
        let mut options = CanOpenOptions {
            nominal_bitrate: 500_000,
            data_bitrate: Some(4_000_000),
            device_name: None,
            listen_only: false,
        };
        assert!(options.validate().is_ok());
        options.listen_only = true;
        assert!(options.validate().is_err());
        options.listen_only = false;
        options.data_bitrate = Some(0);
        assert!(options.validate().is_err());
    }
}

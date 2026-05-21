pub fn selector_supports_can_fd(selector: &str) -> bool {
    selector.starts_with("gs_usb:") || selector.starts_with("peak_usb_fd:")
}

pub fn parse_gs_usb_selector(selector: &str) -> Option<(u16, u16)> {
    let parts: Vec<&str> = selector.strip_prefix("gs_usb:")?.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let vid = u16::from_str_radix(parts[0], 16).ok()?;
    let pid = u16::from_str_radix(parts[1], 16).ok()?;
    Some((vid, pid))
}

pub fn parse_peak_usb_fd_selector(selector: &str) -> Option<(u16, u8, u8, u8)> {
    let parts: Vec<&str> = selector.strip_prefix("peak_usb_fd:")?.split(':').collect();
    if parts.len() != 4 {
        return None;
    }
    let pid = u16::from_str_radix(parts[0], 16).ok()?;
    let bus = parts[1].parse::<u8>().ok()?;
    let addr = parts[2].parse::<u8>().ok()?;
    let channel = parts[3].parse::<u8>().ok()?;
    Some((pid, bus, addr, channel))
}

pub fn selector_matches_device(selector: &str, display_name: &str, device_id: &str) -> bool {
    selector == display_name || selector == device_id
}

pub fn format_windows_device_label(name: &str, vendor: &str, bitness: u8) -> String {
    let name = name.trim();
    let vendor = vendor.trim();
    let mut suffix: Vec<String> = Vec::new();
    if !vendor.is_empty() {
        suffix.push(vendor.to_string());
    }
    if matches!(bitness, 32 | 64) {
        suffix.push(format!("{bitness}-bit"));
    }
    if suffix.is_empty() {
        name.to_string()
    } else {
        format!("{name} [{}]", suffix.join(", "))
    }
}

pub fn disambiguate_windows_device_label(
    base_label: &str,
    dll_path: &str,
    duplicate_count: usize,
) -> String {
    if duplicate_count <= 1 {
        return base_label.to_string();
    }
    let dll_name = dll_path.rsplit(['\\', '/']).next().unwrap_or("");
    if dll_name.is_empty() {
        base_label.to_string()
    } else {
        format!("{base_label} - {dll_name}")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        disambiguate_windows_device_label, format_windows_device_label, parse_gs_usb_selector,
        parse_peak_usb_fd_selector, selector_matches_device, selector_supports_can_fd,
    };

    #[test]
    fn selector_supports_can_fd_matches_known_fd_transports() {
        assert!(selector_supports_can_fd("gs_usb:1d50:606f"));
        assert!(selector_supports_can_fd("peak_usb_fd:0012:1:2:0"));
        assert!(!selector_supports_can_fd("openport:1:2"));
        assert!(!selector_supports_can_fd("toucan:1:2"));
    }

    #[test]
    fn parse_known_selector_shapes() {
        assert_eq!(
            parse_gs_usb_selector("gs_usb:1d50:606f"),
            Some((0x1d50, 0x606f))
        );
        assert_eq!(
            parse_peak_usb_fd_selector("peak_usb_fd:0012:1:2:0"),
            Some((0x0012, 1, 2, 0))
        );
    }

    #[test]
    fn reject_invalid_selector_shapes() {
        assert_eq!(parse_gs_usb_selector("gs_usb:1d50"), None);
        assert_eq!(parse_peak_usb_fd_selector("peak_usb_fd:0012:1"), None);
        assert_eq!(parse_peak_usb_fd_selector("peak_usb_fd:gggg:1:2:0"), None);
    }

    #[test]
    fn selector_matches_display_name_or_device_id() {
        assert!(selector_matches_device(
            "PCAN-USB FD",
            "PCAN-USB FD",
            "peak_usb_fd:0012:1:2:0"
        ));
        assert!(selector_matches_device(
            "peak_usb_fd:0012:1:2:0",
            "PCAN-USB FD",
            "peak_usb_fd:0012:1:2:0"
        ));
        assert!(!selector_matches_device(
            "gs_usb:1d50:606f",
            "PCAN-USB FD",
            "peak_usb_fd:0012:1:2:0"
        ));
    }

    #[test]
    fn windows_label_helpers_preserve_vendor_bitness_and_duplicate_suffix() {
        let base = format_windows_device_label("OpenPort 2.0", "Tactrix", 64);
        assert_eq!(base, "OpenPort 2.0 [Tactrix, 64-bit]");
        assert_eq!(
            disambiguate_windows_device_label(&base, r"C:\Drivers\op20pt32.dll", 2),
            "OpenPort 2.0 [Tactrix, 64-bit] - op20pt32.dll"
        );
        assert_eq!(
            disambiguate_windows_device_label(&base, r"C:\Drivers\op20pt32.dll", 1),
            "OpenPort 2.0 [Tactrix, 64-bit]"
        );
    }
}

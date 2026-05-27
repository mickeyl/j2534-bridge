use rusb::{Context, Device, DeviceDescriptor, Direction, TransferType, UsbContext};
use serde::{Deserialize, Serialize};

const OPENPORT_VENDOR_ID: u16 = 0x0403;
const OPENPORT_PRODUCT_ID: u16 = 0xcc4d;
const OPENPORT_NAME: &str = "Tactrix OpenPort 2.0";
const OPENPORT_VENDOR: &str = "Tactrix";

const TOUCAN_VENDOR_ID: u16 = 0x16D0;
const TOUCAN_PRODUCT_ID: u16 = 0x0EAC;
const TOUCAN_NAME: &str = "TouCAN";
const TOUCAN_VENDOR: &str = "Rusoku";

const PEAK_VENDOR_ID: u16 = 0x0C72;
const PEAK_USB_PRO_FD_PRODUCT_ID: u16 = 0x0011;
const PEAK_USB_FD_PRODUCT_ID: u16 = 0x0012;
const PEAK_USB_PRO_FD_NAME: &str = "PCAN-USB Pro FD";
const PEAK_USB_FD_NAME: &str = "PCAN-USB FD";
const PEAK_VENDOR: &str = "PEAK-System";

const GS_USB_BREQ_HOST_FORMAT: u8 = 0;
const GS_USB_BREQ_BT_CONST: u8 = 4;
const GS_CAN_FEATURE_TERMINATION: u32 = 1 << 11;

const GS_USB_DEVICES: &[(u16, u16, &str, &str)] = &[
    (0x1D50, 0x606F, "CANable/candleLight", "candleLight"),
    (0x1209, 0x2323, "candleLight", "candleLight"),
    (0x1CD2, 0x606F, "CES CANEXT FD", "CES"),
    (0x16D0, 0x10B8, "ABE CANdebugger FD", "ABE"),
    (0x16D0, 0x0F30, "Xylanta SAINT3 / Innomaker", "Xylanta"),
    (0x1209, 0xCA01, "CANnectivity", "CANnectivity"),
    (0x1D50, 0x606D, "CANtact", "candleLight"),
    (0x1D50, 0x6070, "CANable 2.0", "candleLight"),
    (0x1209, 0x0001, "Geschwister Schneider", "GS USB"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct J2534Device {
    pub name: String,
    pub vendor: String,
    pub dll_path: String,
    pub can_iso15765: bool,
    pub can_iso11898: bool,
    pub compatible: bool,
    pub supports_can: bool,
    pub supports_can_fd: bool,
    pub supports_kline: bool,
    pub supports_termination: bool,
    pub bitness: u8,
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub api_version: String,
    pub supported_protocols: Vec<String>,
    pub baud_rates: Vec<u32>,
    pub init_modes: Vec<String>,
}

#[derive(Clone, Copy)]
enum UsbTransferType {
    Bulk,
    Interrupt,
}

struct UsbEndpoints {
    interface: u8,
    #[allow(dead_code)]
    addr_in: u8,
    #[allow(dead_code)]
    addr_out: u8,
    #[allow(dead_code)]
    in_type: UsbTransferType,
    #[allow(dead_code)]
    out_type: UsbTransferType,
}

struct GsUsbBtConst {
    feature: u32,
}

struct GsUsbDevice {
    handle: rusb::DeviceHandle<Context>,
    endpoints: UsbEndpoints,
}

impl GsUsbDevice {
    fn open(vendor_id: u16, product_id: u16) -> Result<Self, String> {
        let context = Context::new().map_err(|e| format!("USB context failed: {e}"))?;
        let list = context
            .devices()
            .map_err(|e| format!("USB device list failed: {e}"))?;

        for device in list.iter() {
            let desc = device
                .device_descriptor()
                .map_err(|e| format!("Descriptor read failed: {e}"))?;
            if desc.vendor_id() != vendor_id || desc.product_id() != product_id {
                continue;
            }

            let handle = device
                .open()
                .map_err(|e| format!("Open device failed: {e}"))?;
            let endpoints = find_usb_endpoints(&device, &desc, true, true)
                .ok_or_else(|| "Failed to find bulk endpoints".to_string())?;
            if handle
                .kernel_driver_active(endpoints.interface)
                .unwrap_or(false)
            {
                let _ = handle.detach_kernel_driver(endpoints.interface);
            }
            handle
                .claim_interface(endpoints.interface)
                .map_err(|e| format!("Claim interface failed: {e}"))?;

            return Ok(Self { handle, endpoints });
        }

        Err("ERR_J2534_DEVICE_NOT_FOUND".to_string())
    }

    fn send_host_format(&mut self) -> Result<(), String> {
        let payload = [0x01_u8, 0x00, 0x00, 0x00];
        self.handle
            .write_control(
                0x41,
                GS_USB_BREQ_HOST_FORMAT,
                0,
                self.endpoints.interface as u16,
                &payload,
                std::time::Duration::from_millis(200),
            )
            .map_err(|e| format!("gs_usb host format failed: {e}"))?;
        Ok(())
    }

    fn get_bt_const(&mut self, channel: u16) -> Result<GsUsbBtConst, String> {
        let mut buf = [0_u8; 40];
        let read = self
            .handle
            .read_control(
                0xC1,
                GS_USB_BREQ_BT_CONST,
                0,
                channel,
                &mut buf,
                std::time::Duration::from_millis(200),
            )
            .map_err(|e| format!("gs_usb BT_CONST read failed: {e}"))?;
        if read < 40 {
            return Err("gs_usb BT_CONST response too short".to_string());
        }
        let feature = u32::from_le_bytes([buf[36], buf[37], buf[38], buf[39]]);
        Ok(GsUsbBtConst { feature })
    }
}

fn transfer_type(endpoint: &rusb::EndpointDescriptor) -> Option<UsbTransferType> {
    match endpoint.transfer_type() {
        TransferType::Bulk => Some(UsbTransferType::Bulk),
        TransferType::Interrupt => Some(UsbTransferType::Interrupt),
        _ => None,
    }
}

fn find_usb_endpoints(
    device: &Device<Context>,
    desc: &DeviceDescriptor,
    require_in: bool,
    require_out: bool,
) -> Option<UsbEndpoints> {
    let mut configs = Vec::new();
    if let Ok(active) = device.active_config_descriptor() {
        configs.push(active);
    }
    for idx in 0..desc.num_configurations() {
        if let Ok(cfg) = device.config_descriptor(idx) {
            configs.push(cfg);
        }
    }

    for config in configs {
        for interface in config.interfaces() {
            for interface_desc in interface.descriptors() {
                if interface_desc.num_endpoints() < 2 {
                    continue;
                }

                let mut addr_in = None;
                let mut addr_out = None;
                let mut in_type = None;
                let mut out_type = None;

                for endpoint in interface_desc.endpoint_descriptors() {
                    let ep_type = transfer_type(&endpoint)?;
                    match endpoint.direction() {
                        Direction::In => {
                            addr_in = Some(endpoint.address());
                            in_type = Some(ep_type);
                        }
                        Direction::Out => {
                            addr_out = Some(endpoint.address());
                            out_type = Some(ep_type);
                        }
                    }
                }

                let have_in = addr_in.is_some() && in_type.is_some();
                let have_out = addr_out.is_some() && out_type.is_some();
                if (require_in && !have_in) || (require_out && !have_out) {
                    continue;
                }
                if have_in {
                    return Some(UsbEndpoints {
                        interface: interface_desc.interface_number(),
                        addr_in: addr_in.unwrap(),
                        addr_out: addr_out.unwrap_or(addr_in.unwrap()),
                        in_type: in_type.unwrap(),
                        out_type: out_type.unwrap_or(in_type.unwrap()),
                    });
                }
            }
        }
    }

    None
}

fn probe_gs_usb_features(vendor_id: u16, product_id: u16) -> Option<u32> {
    let mut device = GsUsbDevice::open(vendor_id, product_id).ok()?;
    device.send_host_format().ok()?;
    let bt_const = device.get_bt_const(0).ok()?;
    let _ = device.handle.release_interface(device.endpoints.interface);
    Some(bt_const.feature)
}

fn supported_protocols(can_iso15765: bool, can_iso11898: bool) -> Vec<String> {
    let mut protocols = Vec::new();
    if can_iso11898 {
        protocols.push("CAN".to_string());
    }
    if can_iso15765 {
        protocols.push("ISO-TP".to_string());
    }
    protocols
}

pub fn enumerate_devices() -> Vec<J2534Device> {
    use std::collections::HashSet;

    let mut devices = Vec::new();
    let mut seen: HashSet<(u8, u8)> = HashSet::new();
    let mut toucan_index: u32 = 0;
    let mut gs_usb_index: u32 = 0;
    let mut openport_index: u32 = 0;

    let context = match Context::new() {
        Ok(ctx) => ctx,
        Err(_) => return devices,
    };
    let list = match context.devices() {
        Ok(list) => list,
        Err(_) => return devices,
    };

    for device in list.iter() {
        let bus = device.bus_number();
        let addr = device.address();
        if seen.contains(&(bus, addr)) {
            continue;
        }

        let desc: DeviceDescriptor = match device.device_descriptor() {
            Ok(desc) => desc,
            Err(_) => continue,
        };

        if desc.vendor_id() == OPENPORT_VENDOR_ID && desc.product_id() == OPENPORT_PRODUCT_ID {
            seen.insert((bus, addr));
            let compatible = find_usb_endpoints(&device, &desc, true, true).is_some();
            let selector = format!("openport:{openport_index}");
            openport_index += 1;
            devices.push(J2534Device {
                name: OPENPORT_NAME.to_string(),
                vendor: OPENPORT_VENDOR.to_string(),
                dll_path: selector,
                can_iso15765: true,
                can_iso11898: true,
                compatible,
                supports_can: true,
                supports_can_fd: false,
                supports_kline: false,
                supports_termination: false,
                bitness: 0,
                available: true,
                unavailable_reason: None,
                api_version: String::new(),
                supported_protocols: supported_protocols(true, true),
                baud_rates: Vec::new(),
                init_modes: Vec::new(),
            });
        } else if desc.vendor_id() == TOUCAN_VENDOR_ID && desc.product_id() == TOUCAN_PRODUCT_ID {
            seen.insert((bus, addr));
            let compatible = find_usb_endpoints(&device, &desc, true, false).is_some();
            let selector = format!("toucan:{toucan_index}");
            toucan_index += 1;
            devices.push(J2534Device {
                name: TOUCAN_NAME.to_string(),
                vendor: TOUCAN_VENDOR.to_string(),
                dll_path: selector,
                can_iso15765: true,
                can_iso11898: true,
                compatible,
                supports_can: true,
                supports_can_fd: false,
                supports_kline: false,
                supports_termination: false,
                bitness: 0,
                available: true,
                unavailable_reason: None,
                api_version: String::new(),
                supported_protocols: supported_protocols(true, true),
                baud_rates: Vec::new(),
                init_modes: Vec::new(),
            });
        } else if desc.vendor_id() == PEAK_VENDOR_ID
            && (desc.product_id() == PEAK_USB_PRO_FD_PRODUCT_ID
                || desc.product_id() == PEAK_USB_FD_PRODUCT_ID)
        {
            seen.insert((bus, addr));
            let base_name = if let Ok(handle) = device.open() {
                handle.read_product_string_ascii(&desc).unwrap_or_else(|_| {
                    if desc.product_id() == PEAK_USB_PRO_FD_PRODUCT_ID {
                        PEAK_USB_PRO_FD_NAME.to_string()
                    } else {
                        PEAK_USB_FD_NAME.to_string()
                    }
                })
            } else if desc.product_id() == PEAK_USB_PRO_FD_PRODUCT_ID {
                PEAK_USB_PRO_FD_NAME.to_string()
            } else {
                PEAK_USB_FD_NAME.to_string()
            };
            let channel_count = if desc.product_id() == PEAK_USB_PRO_FD_PRODUCT_ID {
                2
            } else {
                1
            };
            for channel in 0..channel_count {
                let channel_name = if channel_count > 1 {
                    format!("{base_name} CAN {}", channel + 1)
                } else {
                    base_name.clone()
                };
                devices.push(J2534Device {
                    name: channel_name,
                    vendor: PEAK_VENDOR.to_string(),
                    dll_path: format!(
                        "peak_usb_fd:{:04x}:{bus}:{addr}:{channel}",
                        desc.product_id()
                    ),
                    can_iso15765: false,
                    can_iso11898: true,
                    compatible: true,
                    supports_can: true,
                    supports_can_fd: true,
                    supports_kline: false,
                    supports_termination: false,
                    bitness: 0,
                    available: true,
                    unavailable_reason: None,
                    api_version: "direct USB".to_string(),
                    supported_protocols: supported_protocols(false, true),
                    baud_rates: Vec::new(),
                    init_modes: Vec::new(),
                });
            }
        } else {
            for &(vid, pid, name, vendor) in GS_USB_DEVICES {
                if desc.vendor_id() == vid && desc.product_id() == pid {
                    seen.insert((bus, addr));
                    let compatible = find_usb_endpoints(&device, &desc, true, true).is_some();
                    let display_name = if let Ok(handle) = device.open() {
                        handle
                            .read_product_string_ascii(&desc)
                            .unwrap_or_else(|_| name.to_string())
                    } else {
                        name.to_string()
                    };
                    let supports_termination = probe_gs_usb_features(vid, pid)
                        .map(|features| (features & GS_CAN_FEATURE_TERMINATION) != 0)
                        .unwrap_or(false);

                    let selector = format!("gs_usb:{gs_usb_index}");
                    gs_usb_index += 1;
                    devices.push(J2534Device {
                        name: display_name,
                        vendor: vendor.to_string(),
                        dll_path: selector,
                        can_iso15765: false,
                        can_iso11898: true,
                        compatible,
                        supports_can: true,
                        supports_can_fd: true,
                        supports_kline: false,
                        supports_termination,
                        bitness: 0,
                        available: true,
                        unavailable_reason: None,
                        api_version: String::new(),
                        supported_protocols: supported_protocols(false, true),
                        baud_rates: Vec::new(),
                        init_modes: Vec::new(),
                    });
                    break;
                }
            }
        }
    }

    devices
}

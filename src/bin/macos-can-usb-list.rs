#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("macos-can-usb-list is only supported on macOS");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use clap::Parser;
    use j2534_bridge::macos;

    #[derive(Parser, Debug)]
    #[command(name = "macos-can-usb-list")]
    #[command(about = "List macOS USB CAN adapters detected by the shared inventory crate")]
    struct Cli {
        #[arg(long, help = "Emit the raw inventory records as JSON")]
        json: bool,
    }

    let cli = Cli::parse();
    let mut devices = macos::enumerate_devices();
    devices.sort_by(|a, b| {
        a.dll_path
            .cmp(&b.dll_path)
            .then_with(|| a.name.cmp(&b.name))
    });

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&devices)?);
        return Ok(());
    }

    if devices.is_empty() {
        println!("No macOS CAN USB devices found.");
        return Ok(());
    }

    println!("macOS CAN USB devices:");
    for device in devices {
        let protocols = if device.supported_protocols.is_empty() {
            "-".to_string()
        } else {
            device.supported_protocols.join(",")
        };
        let status = match (
            device.available,
            device.compatible,
            device.unavailable_reason.as_deref(),
        ) {
            (true, true, _) => "available".to_string(),
            (true, false, _) => "available, incompatible endpoints".to_string(),
            (false, _, Some(reason)) => format!("unavailable: {reason}"),
            (false, _, None) => "unavailable".to_string(),
        };
        let termination = if device.supports_termination {
            "yes"
        } else {
            "no"
        };

        println!(
            "- {} ({}) selector={} protocols={} termination={} status={}",
            device.name, device.vendor, device.dll_path, protocols, termination, status
        );
    }

    Ok(())
}

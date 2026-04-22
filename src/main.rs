use std::io::{self, BufRead};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use log::{error, info};

use antneuro::prelude::*;
use antneuro::client::AntNeuroConfig;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("antneuro - ANT Neuro EEG recorder");
        println!();
        println!("Supported devices: eego mylab, eego sport, eego rt, waveguard (via eego)");
        println!();
        println!("Usage: antneuro [OPTIONS]");
        println!();
        println!("Options:");
        println!("  --lib <PATH>       Path to eego SDK shared library");
        println!("  --rate <HZ>        Sampling rate (default: 500)");
        println!("  --impedance        Open impedance stream instead of EEG");
        println!("  --output <PATH>    Record data to CSV file");
        println!("  --duration <SECS>  Recording duration in seconds");
        println!("  --list             List connected amplifiers and exit");
        println!("  --usb-scan         Scan for eego USB devices (no SDK needed, works on macOS)");
        println!("  --usb-dump         Dump USB descriptors for connected eego devices");
        println!("  -h, --help         Show this help");
        println!();
        println!("Interactive commands (type + Enter while streaming):");
        println!("  q  - quit");
        println!("  p  - pause streaming");
        println!("  r  - resume streaming");
        println!("  i  - show amplifier info");
        return Ok(());
    }

    let library_path = args
        .iter()
        .position(|a| a == "--lib")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);

    let sampling_rate: i32 = args
        .iter()
        .position(|a| a == "--rate")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);

    let impedance = args.iter().any(|a| a == "--impedance");

    let output_csv = args
        .iter()
        .position(|a| a == "--output")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);

    let duration_secs: Option<u64> = args
        .iter()
        .position(|a| a == "--duration")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok());

    let mut config = AntNeuroConfig::default();
    if let Some(p) = library_path {
        config.library_path = p;
    }
    config.sampling_rate = sampling_rate;
    config.impedance_mode = impedance;

    // ── USB scan mode (requires `native` feature) ─────────────────────────
    #[cfg(feature = "native")]
    {
        if args.iter().any(|a| a == "--usb-scan") {
            let devices = antneuro::usb::find_eego_devices()?;
            if devices.is_empty() {
                println!("No eego USB devices found.");
                println!("  Make sure the amplifier is connected via USB and powered on.");
            } else {
                println!("Found {} eego USB device(s):", devices.len());
                for d in &devices {
                    println!("  Bus {:03} Device {:03}: {:04x}:{:04x}", d.bus, d.address, d.vendor_id, d.product_id);
                    if !d.manufacturer.is_empty() { println!("    Manufacturer: {}", d.manufacturer); }
                    if !d.product.is_empty() { println!("    Product: {}", d.product); }
                    if !d.serial.is_empty() { println!("    Serial: {}", d.serial); }
                }
            }
            return Ok(());
        }

        if args.iter().any(|a| a == "--usb-dump") {
            antneuro::usb::dump_eego_descriptors()?;
            return Ok(());
        }
    }

    // ── List mode ────────────────────────────────────────────────────────────
    if args.iter().any(|a| a == "--list") {
        let sdk = AntNeuroSdk::new(&config.library_path)?;
        let infos = sdk.get_amplifiers_info()?;
        if infos.is_empty() {
            println!("No amplifiers found.");
        } else {
            println!("Connected amplifiers:");
            for info in &infos {
                println!("  id={} serial={}", info.id, info.serial);
                // Open and show details
                if let Ok(amp) = sdk.open_amplifier(info.id) {
                    if let Ok(t) = amp.amplifier_type() {
                        println!("    type={}", t);
                    }
                    if let Ok(fw) = amp.firmware_version() {
                        println!("    firmware={}", fw);
                    }
                    if let Ok(rates) = amp.sampling_rates_available() {
                        println!("    sampling_rates={:?}", rates);
                    }
                    if let Ok(channels) = amp.channel_list() {
                        println!("    channels={} ({} types)", channels.len(),
                            channels.iter().map(|c| format!("{}", c.channel_type)).collect::<std::collections::BTreeSet<_>>().into_iter().collect::<Vec<_>>().join(", "));
                    }
                    if let Ok(ps) = amp.power_state() {
                        println!("    power: powered={} charging={} level={}%",
                            ps.is_powered, ps.is_charging, ps.charging_level);
                    }
                }
            }
        }
        return Ok(());
    }

    // ── Start client ─────────────────────────────────────────────────────────
    let client = antneuro::client::AntNeuroClient::new(config.clone());
    let (mut rx, handle) = client.start()?;
    let handle = Arc::new(handle);

    info!("Streaming started. Press Ctrl-C or type 'q' + Enter to quit.");
    info!("Commands (type + Enter):");
    info!("  q  - quit");
    info!("  p  - pause streaming");
    info!("  r  - resume streaming");
    info!("  i  - show amplifier info");

    // ── Stdin command loop ────────────────────────────────────────────────────
    let (line_tx, mut line_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    std::thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(l) => {
                    if line_tx.send(l.trim().to_owned()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let handle_cmd = Arc::clone(&handle);
    tokio::spawn(async move {
        while let Some(line) = line_rx.recv().await {
            if line.is_empty() {
                continue;
            }
            match line.as_str() {
                "q" => {
                    info!("Quit requested.");
                    handle_cmd.disconnect();
                    std::process::exit(0);
                }
                "p" => {
                    info!("Pausing ...");
                    handle_cmd.pause();
                }
                "r" => {
                    info!("Resuming ...");
                    handle_cmd.resume();
                }
                "i" => {
                    info!("Use --list flag to show amplifier details.");
                }
                cmd => {
                    error!("Unknown command: '{cmd}'. Available: q, p, r, i");
                }
            }
        }
    });

    // ── Main event loop ──────────────────────────────────────────────────────
    let mut recorder: Option<CsvRecorder> = None;
    let mut total_samples: u64 = 0;

    let deadline = duration_secs.map(|s| {
        tokio::time::Instant::now() + std::time::Duration::from_secs(s)
    });

    println!(
        "Starting ANT Neuro recording (rate={}Hz, mode={})...",
        sampling_rate,
        if impedance { "impedance" } else { "EEG" },
    );

    loop {
        let event = if let Some(dl) = deadline {
            tokio::select! {
                ev = rx.recv() => ev,
                _ = tokio::time::sleep_until(dl) => {
                    println!("Duration reached. Stopping.");
                    break;
                }
            }
        } else {
            rx.recv().await
        };

        match event {
            Some(AntNeuroEvent::Connected(info)) => {
                info!("Connected: id={} serial={}", info.id, info.serial);
            }
            Some(AntNeuroEvent::Eeg(data)) => {
                total_samples += data.sample_count as u64;

                if recorder.is_none() {
                    if let Some(ref path) = output_csv {
                        recorder = Some(CsvRecorder::new(path, &data.channels)?);
                        info!("Recording to {}", path.display());
                    }
                }
                if let Some(ref mut rec) = recorder {
                    rec.write_block(
                        data.channel_count,
                        data.sample_count,
                        &data.samples,
                        config.sampling_rate as f64,
                    )?;
                }

                if total_samples % 1000 < data.sample_count as u64 {
                    let first = data.samples.first().copied().unwrap_or(0.0);
                    println!(
                        "[EEG] samples={} channels={} first={:.6}V total={}",
                        data.sample_count, data.channel_count, first, total_samples
                    );
                }
            }
            Some(AntNeuroEvent::Impedance(data)) => {
                total_samples += data.sample_count as u64;
                if recorder.is_none() {
                    if let Some(ref path) = output_csv {
                        recorder = Some(CsvRecorder::new(path, &data.channels)?);
                    }
                }
                if let Some(ref mut rec) = recorder {
                    rec.write_block(
                        data.channel_count,
                        data.sample_count,
                        &data.samples,
                        1.0,
                    )?;
                }
                println!(
                    "[IMPEDANCE] channels={} first={:.1} Ohm",
                    data.channel_count,
                    data.samples.first().copied().unwrap_or(0.0),
                );
            }
            Some(AntNeuroEvent::Error(e)) => {
                error!("Error: {}", e);
            }
            Some(AntNeuroEvent::Disconnected) | None => {
                info!("Disconnected. Total samples: {}", total_samples);
                break;
            }
        }
    }

    if let Some(ref mut rec) = recorder {
        rec.flush()?;
        info!("Recording saved.");
    }

    Ok(())
}

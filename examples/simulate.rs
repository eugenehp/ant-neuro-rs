//! End-to-end test of the simulated eego pipeline.
//! Runs without any USB hardware.
//!
//! Usage: cargo run --example simulate

use antneuro::prelude::*;
use antneuro::simulator::SimulatorConfig;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Create SDK with simulated amplifier
    let sdk = AntNeuroSdk::new_simulated(vec![
        SimulatorConfig {
            serial: "EE225-0001-SIM".to_string(),
            ref_channels: 32,
            bip_channels: 0,
            sampling_rate: 500,
            firmware_version: 200,
            battery_level: 85,
        },
    ])?;

    println!("SDK version: {}", sdk.version());

    // Discover
    let infos = sdk.get_amplifiers_info()?;
    println!("Found {} amplifier(s):", infos.len());
    for info in &infos {
        println!("  id={} serial={}", info.id, info.serial);
    }

    // Open
    let amp = sdk.open_amplifier(infos[0].id)?;
    println!("\nAmplifier type: {}", amp.amplifier_type()?);
    println!("Serial: {}", amp.serial()?);
    println!("Firmware: {}", amp.firmware_version()?);

    let ps = amp.power_state()?;
    println!("Power: powered={} charging={} level={}%", ps.is_powered, ps.is_charging, ps.charging_level);

    let channels = amp.channel_list()?;
    println!("Channels: {}", channels.len());
    for ch in &channels[..5.min(channels.len())] {
        println!("  [{}] {}", ch.index, ch.channel_type);
    }
    if channels.len() > 5 {
        println!("  ... and {} more", channels.len() - 5);
    }

    println!("Sampling rates: {:?}", amp.sampling_rates_available()?);
    println!("Reference ranges: {:?}", amp.reference_ranges_available()?);
    println!("Bipolar ranges: {:?}", amp.bipolar_ranges_available()?);

    // Stream EEG
    println!("\n--- Starting EEG stream at 500 Hz ---");
    let stream = amp.open_eeg_stream_default(500)?;
    println!("Stream channels: {}", stream.channel_count());

    let mut total_samples = 0u64;
    let start = std::time::Instant::now();

    println!("Stream channel count: {}", stream.channel_count());
    for block in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(100));

        let prefetch = stream.prefetch()?;
        println!("  prefetch={} bytes ({} doubles, {} samples)", prefetch, prefetch/8, prefetch/8/stream.channel_count());

        if let Some((ch_count, sample_count, data)) = stream.get_data()? {
            total_samples += sample_count as u64;
            if block % 5 == 0 {
                let first_ch0 = data.first().copied().unwrap_or(0.0);
                println!(
                    "  block {:2}: {} samples x {} ch, ch0[0]={:+.3} uV, total={}",
                    block, sample_count, ch_count,
                    first_ch0 * 1e6, // Volts → µV
                    total_samples,
                );
            }
        }
    }

    let elapsed = start.elapsed();
    let rate = total_samples as f64 / elapsed.as_secs_f64();
    println!(
        "\n{} total samples in {:.1}s = {:.0} samples/sec ({:.0} Hz effective)",
        total_samples, elapsed.as_secs_f64(), rate, rate
    );

    drop(stream);
    println!("\nStream closed. Done.");

    Ok(())
}

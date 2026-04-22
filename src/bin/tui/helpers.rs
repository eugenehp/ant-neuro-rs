//! Constants, formatting helpers, smooth-signal filter, and the built-in
//! EEG simulator used by `--simulate`.

use std::f64::consts::PI;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ratatui::style::Color;

use super::app::App;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Width of the scrolling waveform window in seconds.
pub(crate) const WINDOW_SECS: f64 = 2.0;

/// Default EEG sample rate (Hz). Updated from the actual amplifier.
pub(crate) const DEFAULT_HZ: f64 = 500.0;

/// Maximum number of EEG channels to display in the TUI.
pub(crate) const MAX_DISPLAY_CH: usize = 8;

/// Discrete Y-axis scale steps in Volts (half the full symmetric range).
/// eego SDK returns data in Volts, not uV.
pub(crate) const Y_SCALES: &[f64] = &[
    0.00001, 0.000025, 0.00005, 0.0001, 0.0002, 0.0005, 0.001, 0.002, 0.01,
];

/// Index into `Y_SCALES` used when the app starts.
pub(crate) const DEFAULT_SCALE: usize = 5;

/// Per-channel line colours.
pub(crate) const COLORS: [Color; 8] = [
    Color::Cyan,
    Color::Yellow,
    Color::Green,
    Color::Magenta,
    Color::LightRed,
    Color::LightBlue,
    Color::White,
    Color::LightGreen,
];

/// Dimmed versions for smooth overlay background.
pub(crate) const DIM_COLORS: [Color; 8] = [
    Color::Rgb(0, 90, 110),
    Color::Rgb(110, 90, 0),
    Color::Rgb(0, 110, 0),
    Color::Rgb(110, 0, 110),
    Color::Rgb(110, 40, 40),
    Color::Rgb(0, 60, 90),
    Color::Rgb(80, 80, 80),
    Color::Rgb(0, 80, 0),
];

/// Moving-average window in samples.
pub(crate) const SMOOTH_WINDOW: usize = 9;

/// Braille spinner frames.
pub(crate) const SPINNER: &[&str] = &["\u{280b}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283c}", "\u{2834}", "\u{2826}", "\u{2827}", "\u{2807}", "\u{280f}"];

/// Seconds between automatic retry scans.
pub(crate) const RETRY_SECS: u64 = 3;

// ── Helper functions ─────────────────────────────────────────────────────────

pub(crate) fn smooth_signal(data: &[(f64, f64)], window: usize) -> Vec<(f64, f64)> {
    if data.len() < 3 || window < 2 {
        return data.to_vec();
    }
    let half = window / 2;
    data.iter()
        .enumerate()
        .map(|(i, &(x, _))| {
            let start = i.saturating_sub(half);
            let end = (i + half + 1).min(data.len());
            let sum: f64 = data[start..end].iter().map(|&(_, y)| y).sum();
            (x, sum / (end - start) as f64)
        })
        .collect()
}

pub(crate) fn format_voltage(v: f64) -> String {
    let abs = v.abs();
    if abs >= 0.001 {
        format!("{:+.3} mV", v * 1000.0)
    } else {
        format!("{:+.1} uV", v * 1_000_000.0)
    }
}

// ── EEG simulator ─────────────────────────────────────────────────────────────

fn sim_sample(t: f64, ch: usize) -> f64 {
    let phi = ch as f64 * PI / 2.5;
    let alpha = 20e-6 * (2.0 * PI * 10.0 * t + phi).sin();
    let beta = 6e-6 * (2.0 * PI * 22.0 * t + phi * 1.7).sin();
    let theta = 10e-6 * (2.0 * PI * 6.0 * t + phi * 0.9).sin();
    let nx = t * 1000.7 + ch as f64 * 137.508;
    let noise = ((nx.sin() * 9973.1).fract() - 0.5) * 8e-6;
    alpha + beta + theta + noise
}

pub(crate) fn spawn_simulator(app: Arc<Mutex<App>>) {
    let num_ch = {
        let s = app.lock().unwrap();
        s.num_channels
    };
    let hz = DEFAULT_HZ;
    let samples_per_pkt = 12;
    tokio::spawn(async move {
        let pkt_interval = Duration::from_secs_f64(samples_per_pkt as f64 / hz);
        let mut ticker = tokio::time::interval(pkt_interval);
        let dt = 1.0 / hz;
        let mut t = 0.0_f64;
        let mut seq = 0u32;
        loop {
            ticker.tick().await;
            let mut s = app.lock().unwrap();
            if s.paused {
                t += samples_per_pkt as f64 * dt;
                continue;
            }
            for ch in 0..num_ch {
                let samples: Vec<f64> = (0..samples_per_pkt)
                    .map(|i| sim_sample(t + i as f64 * dt, ch))
                    .collect();
                s.push(ch, &samples);
            }
            seq = seq.wrapping_add(1);
            if seq % 21 == 0 {
                s.battery_level = Some(((85.0 - t as f32 / 300.0).clamp(0.0, 100.0)) as i32);
                s.is_charging = Some(false);
            }
            t += samples_per_pkt as f64 * dt;
        }
    });
}

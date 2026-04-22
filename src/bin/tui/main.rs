//! Real-time EEG chart viewer for ANT Neuro eego amplifiers.
//!
//! Usage:
//!   cargo run --bin tui -- --lib path/to/libeego-SDK.so
//!   cargo run --bin tui -- --simulate   # built-in EEG simulator (no hardware needed)
//!
//! Keys (streaming view)
//! ---------------------
//!   Tab      open device picker
//!   +  / =   zoom out  (increase V scale)
//!   -        zoom in   (decrease V scale)
//!   a        auto-scale: fit Y axis to current peak amplitude
//!   v        toggle smooth overlay (dim raw + bright 9-pt moving-average)
//!   p        pause streaming
//!   r        resume streaming
//!   c        clear waveform buffers
//!   d        disconnect current amplifier
//!   q / Esc  quit
//!
//! Keys (device picker overlay)
//! ----------------------------
//!   Up/Down  navigate list
//!   Enter    connect to highlighted amplifier
//!   Esc      close picker

mod app;
mod connect;
mod helpers;
mod render;

use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use antneuro::prelude::*;

use app::AppMode;
use connect::{
    device_entry, restart_scan, spawn_event_task, start_connect, start_scan, ConnectOutcome,
    ScanResult,
};
use helpers::{spawn_simulator, RETRY_SECS};

// ── Entry point ──────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    use std::io::IsTerminal as _;
    if !io::stdout().is_terminal() {
        eprintln!("Error: antneuro tui requires a real terminal (TTY).");
        std::process::exit(1);
    }

    {
        use std::fs::File;
        if let Ok(file) = File::create("ant-neuro-tui.log") {
            env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
                .target(env_logger::Target::Pipe(Box::new(file)))
                .init();
        }
    }

    let args: Vec<String> = std::env::args().collect();
    let simulate = args.iter().any(|a| a == "--simulate");

    let lib_path = args
        .iter()
        .position(|a| a == "--lib")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            if cfg!(target_os = "linux") {
                std::path::PathBuf::from("libeego-SDK.so")
            } else if cfg!(target_os = "windows") {
                std::path::PathBuf::from("eego-SDK.dll")
            } else {
                std::path::PathBuf::from("libeego-SDK.dylib")
            }
        });

    let sampling_rate: i32 = args
        .iter()
        .position(|a| a == "--rate")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);

    let app = Arc::new(Mutex::new(app::App::new(4, sampling_rate as f64)));

    let mut amplifier_infos: Vec<AmplifierInfo> = vec![];
    let mut _connected_idx: Option<usize> = None;
    let mut handle: Option<antneuro::client::AntNeuroHandle> = None;

    let mut pending_scan: Option<tokio::sync::oneshot::Receiver<ScanResult>> = None;
    let mut pending_connect: Option<tokio::sync::oneshot::Receiver<Option<ConnectOutcome>>> = None;
    let mut retry_at: Option<tokio::time::Instant> = None;

    let config = antneuro::client::AntNeuroConfig {
        library_path: lib_path.clone(),
        sampling_rate,
        ..Default::default()
    };

    if simulate {
        let mut s = app.lock().unwrap();
        s.mode = AppMode::Simulated;
        s.scale_idx = 2; // smallest scale for simulated uV-range signals
        drop(s);
        spawn_simulator(Arc::clone(&app));
    } else {
        app.lock().unwrap().picker_scanning = true;
        pending_scan = Some(start_scan(lib_path.clone()));
    }

    // ── Terminal setup ────────────────────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    let tick = Duration::from_millis(33); // ~30 FPS

    // ── Main loop ─────────────────────────────────────────────────────────────
    'main: loop {
        // ── 1. Collect finished scan results ─────────────────────────────────
        if let Some(ref mut rx) = pending_scan {
            if let Ok(scan_result) = rx.try_recv() {
                pending_scan = None;
                amplifier_infos = scan_result.infos;

                {
                    let mut s = app.lock().unwrap();
                    s.picker_entries = amplifier_infos.iter().map(device_entry).collect();
                    s.picker_scanning = false;
                    if amplifier_infos.is_empty() {
                        s.mode = AppMode::NoDevices;
                        if let Some(err) = scan_result.error {
                            s.last_error = Some(err);
                        }
                    }
                }

                if amplifier_infos.is_empty() {
                    retry_at = Some(
                        tokio::time::Instant::now() + Duration::from_secs(RETRY_SECS),
                    );
                } else if handle.is_none() && pending_connect.is_none() {
                    pending_connect = Some(start_connect(
                        0,
                        amplifier_infos[0].clone(),
                        Arc::clone(&app),
                        config.clone(),
                    ));
                }
            }
        }

        // ── 1b. Collect finished connection attempt ──────────────────────────
        if let Some(ref mut rx) = pending_connect {
            if let Ok(result) = rx.try_recv() {
                pending_connect = None;
                if let Some(outcome) = result {
                    {
                        let mut s = app.lock().unwrap();
                        s.mode = AppMode::Connected {
                            serial: outcome.serial.clone(),
                        };
                        s.last_error = None;
                        s.picker_connected_idx = Some(outcome.device_idx);
                    }
                    _connected_idx = Some(outcome.device_idx);
                    handle = Some(outcome.handle);
                    spawn_event_task(outcome.rx, Arc::clone(&app));
                } else {
                    _connected_idx = None;
                    amplifier_infos.clear();
                    restart_scan(&app, &mut pending_scan, &mut retry_at, RETRY_SECS);
                }
            }
        }

        // ── 2. Detect unexpected disconnection ──────────────────────────────
        {
            let is_disconnected =
                matches!(app.lock().unwrap().mode, AppMode::Disconnected);
            if is_disconnected && handle.is_some() {
                if let Some(h) = handle.take() {
                    h.disconnect();
                }
                _connected_idx = None;
                amplifier_infos.clear();
                restart_scan(&app, &mut pending_scan, &mut retry_at, RETRY_SECS);
            }
        }

        // ── 3. Fire pending retry scan ───────────────────────────────────────
        if let Some(t) = retry_at {
            if tokio::time::Instant::now() >= t && pending_scan.is_none() {
                retry_at = None;
                app.lock().unwrap().mode = AppMode::Scanning;
                app.lock().unwrap().picker_scanning = true;
                pending_scan = Some(start_scan(lib_path.clone()));
            }
        }

        // ── 4. Render ────────────────────────────────────────────────────────
        {
            let s = app.lock().unwrap();
            terminal.draw(|f| render::draw(f, &s))?;
        }

        // ── 5. Handle keyboard ───────────────────────────────────────────────
        if !event::poll(tick)? {
            continue;
        }
        let Event::Key(key_event) = event::read()? else {
            continue;
        };

        let ctrl_c = key_event.modifiers.contains(KeyModifiers::CONTROL)
            && key_event.code == KeyCode::Char('c');
        if key_event.code == KeyCode::Char('q') || ctrl_c {
            break 'main;
        }

        // ── Picker overlay keys ──────────────────────────────────────────────
        if app.lock().unwrap().show_picker {
            match key_event.code {
                KeyCode::Esc => {
                    app.lock().unwrap().show_picker = false;
                }
                KeyCode::Up => {
                    let mut s = app.lock().unwrap();
                    if s.picker_cursor > 0 {
                        s.picker_cursor -= 1;
                    }
                }
                KeyCode::Down => {
                    let mut s = app.lock().unwrap();
                    let max = s.picker_entries.len().saturating_sub(1);
                    if s.picker_cursor < max {
                        s.picker_cursor += 1;
                    }
                }
                KeyCode::Enter => {
                    let (cursor, n) = {
                        let s = app.lock().unwrap();
                        (s.picker_cursor, s.picker_entries.len())
                    };
                    if n > 0 && cursor < n && cursor < amplifier_infos.len() {
                        retry_at = None;
                        if let Some(h) = handle.take() {
                            h.disconnect();
                        }
                        _connected_idx = None;
                        pending_connect = Some(start_connect(
                            cursor,
                            amplifier_infos[cursor].clone(),
                            Arc::clone(&app),
                            config.clone(),
                        ));
                    }
                }
                _ => {}
            }
            continue;
        }

        // ── Normal-view keys ─────────────────────────────────────────────────
        match key_event.code {
            KeyCode::Char('q') | KeyCode::Esc => break 'main,

            KeyCode::Tab => {
                let mut s = app.lock().unwrap();
                s.show_picker = true;
                if let Some(ci) = _connected_idx {
                    s.picker_cursor = ci;
                }
            }

            KeyCode::Char('+') | KeyCode::Char('=') => {
                app.lock().unwrap().scale_up();
            }
            KeyCode::Char('-') => {
                app.lock().unwrap().scale_down();
            }
            KeyCode::Char('a') => {
                app.lock().unwrap().auto_scale();
            }

            KeyCode::Char('v') => {
                let mut s = app.lock().unwrap();
                s.smooth = !s.smooth;
            }

            KeyCode::Char('p') => {
                app.lock().unwrap().paused = true;
                if let Some(ref h) = handle {
                    h.pause();
                }
            }
            KeyCode::Char('r') => {
                app.lock().unwrap().paused = false;
                if let Some(ref h) = handle {
                    h.resume();
                }
            }

            KeyCode::Char('c') if !key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                app.lock().unwrap().clear();
            }

            KeyCode::Char('d') => {
                if let Some(h) = handle.take() {
                    h.disconnect();
                }
                pending_connect = None;
                _connected_idx = None;
                app.lock().unwrap().picker_connected_idx = None;
                if pending_scan.is_none() {
                    retry_at = None;
                    app.lock().unwrap().mode = AppMode::Scanning;
                    app.lock().unwrap().picker_scanning = true;
                    pending_scan = Some(start_scan(lib_path.clone()));
                }
            }

            _ => {}
        }
    }

    // ── Teardown ──────────────────────────────────────────────────────────────
    if let Some(h) = handle {
        h.disconnect();
    }
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

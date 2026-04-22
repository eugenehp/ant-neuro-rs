//! Device discovery (scan) and connection logic.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use antneuro::prelude::*;

use super::app::{App, AppMode};
use super::helpers::MAX_DISPLAY_CH;

// ── Scan ─────────────────────────────────────────────────────────────────────

pub(crate) struct ScanResult {
    pub infos: Vec<AmplifierInfo>,
    pub error: Option<String>,
}

pub(crate) fn start_scan(lib_path: std::path::PathBuf) -> tokio::sync::oneshot::Receiver<ScanResult> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let result = match AntNeuroSdk::new(&lib_path) {
            Ok(sdk) => match sdk.get_amplifiers_info() {
                Ok(infos) => {
                    log::info!("Scan completed: {} amplifier(s) found", infos.len());
                    ScanResult { infos, error: None }
                }
                Err(e) => {
                    log::error!("Scan failed: {e}");
                    ScanResult {
                        infos: vec![],
                        error: Some(format!("{e}")),
                    }
                }
            },
            Err(e) => {
                log::error!("SDK load failed: {e}");
                ScanResult {
                    infos: vec![],
                    error: Some(format!("{e}")),
                }
            }
        };
        let _ = tx.send(result);
    });
    rx
}

pub(crate) fn device_entry(info: &AmplifierInfo) -> String {
    format!("id={}  serial={}", info.id, info.serial)
}

pub(crate) fn restart_scan(
    app: &Arc<Mutex<App>>,
    pending_scan: &mut Option<tokio::sync::oneshot::Receiver<ScanResult>>,
    retry_at: &mut Option<tokio::time::Instant>,
    delay_secs: u64,
) {
    {
        let mut s = app.lock().unwrap();
        s.clear();
        s.picker_connected_idx = None;
        s.picker_entries.clear();
        s.show_picker = false;
        s.mode = AppMode::Scanning;
        s.picker_scanning = true;
    }
    if pending_scan.is_none() {
        *retry_at = Some(tokio::time::Instant::now() + Duration::from_secs(delay_secs));
    }
}

// ── Connection ───────────────────────────────────────────────────────────────

pub(crate) struct ConnectOutcome {
    pub rx: tokio::sync::mpsc::Receiver<AntNeuroEvent>,
    pub handle: antneuro::client::AntNeuroHandle,
    pub device_idx: usize,
    pub serial: String,
}

pub(crate) fn start_connect(
    idx: usize,
    info: AmplifierInfo,
    app: Arc<Mutex<App>>,
    config: antneuro::client::AntNeuroConfig,
) -> tokio::sync::oneshot::Receiver<Option<ConnectOutcome>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let serial = info.serial.clone();

    {
        let mut s = app.lock().unwrap();
        s.clear();
        s.mode = AppMode::Connecting(serial.clone());
        s.picker_connected_idx = None;
        s.show_picker = false;
    }

    tokio::spawn(async move {
        let client = antneuro::client::AntNeuroClient::new(config);
        match client.start() {
            Ok((evt_rx, handle)) => {
                let _ = tx.send(Some(ConnectOutcome {
                    rx: evt_rx,
                    handle,
                    device_idx: idx,
                    serial,
                }));
            }
            Err(e) => {
                log::warn!("connect failed: {e}");
                let mut s = app.lock().unwrap();
                s.mode = AppMode::Disconnected;
                s.last_error = Some(format!("{e}"));
                let _ = tx.send(None);
            }
        }
    });

    rx
}

pub(crate) fn spawn_event_task(
    mut rx: tokio::sync::mpsc::Receiver<AntNeuroEvent>,
    app: Arc<Mutex<App>>,
) {
    tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            let mut s = app.lock().unwrap();
            match ev {
                AntNeuroEvent::Connected(info) => {
                    log::info!("Connected: serial={}", info.serial);
                }
                AntNeuroEvent::Disconnected => {
                    s.mode = AppMode::Disconnected;
                    s.picker_connected_idx = None;
                    break;
                }
                AntNeuroEvent::Eeg(data) => {
                    // Update channel labels on first data
                    if s.channel_labels.first().map(|l| l.starts_with("CH")).unwrap_or(true) {
                        let labels: Vec<String> = data
                            .channels
                            .iter()
                            .map(|c| format!("{}_{}", c.channel_type, c.index))
                            .collect();
                        s.set_channel_labels(labels);
                    }
                    // Push per-channel data
                    for sample_idx in 0..data.sample_count {
                        for ch in 0..data.channel_count.min(MAX_DISPLAY_CH) {
                            let val = data.samples[sample_idx * data.channel_count + ch];
                            s.push(ch, &[val]);
                        }
                    }
                }
                AntNeuroEvent::Impedance(_) => {}
                AntNeuroEvent::Error(e) => {
                    s.last_error = Some(e);
                }
            }
        }
    });
}

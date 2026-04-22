use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::error::Result;
use crate::sdk::AntNeuroSdk;
use crate::types::{AntNeuroEvent, EegData, ImpedanceData};

/// Configuration for the ANT Neuro client.
#[derive(Debug, Clone)]
pub struct AntNeuroConfig {
    /// Path to the eego SDK shared library.
    pub library_path: PathBuf,
    /// Sampling rate in Hz (e.g. 500, 512, 1000, 1024, 2000, 2048).
    pub sampling_rate: i32,
    /// Reference channel range in Volts. None = use first available.
    pub reference_range: Option<f64>,
    /// Bipolar channel range in Volts. None = use first available.
    pub bipolar_range: Option<f64>,
    /// Polling interval for data reads.
    pub poll_interval: Duration,
    /// If true, open impedance stream instead of EEG.
    pub impedance_mode: bool,
}

impl Default for AntNeuroConfig {
    fn default() -> Self {
        Self {
            library_path: default_library_path(),
            sampling_rate: 500,
            reference_range: None,
            bipolar_range: None,
            poll_interval: Duration::from_millis(10),
            impedance_mode: false,
        }
    }
}

fn default_library_path() -> PathBuf {
    if cfg!(target_os = "linux") {
        PathBuf::from("libeego-SDK.so")
    } else if cfg!(target_os = "windows") {
        PathBuf::from("eego-SDK.dll")
    } else {
        PathBuf::from("libeego-SDK.dylib")
    }
}

/// Handle for controlling a running stream (pause/resume/disconnect).
pub struct AntNeuroHandle {
    paused: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

impl AntNeuroHandle {
    /// Pause data streaming. The stream stays open but events stop being sent.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    /// Resume data streaming after a pause.
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    /// Check if currently paused.
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Disconnect and stop the streaming task.
    pub fn disconnect(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// High-level async client that streams events, similar to muse-rs MuseClient.
pub struct AntNeuroClient {
    config: AntNeuroConfig,
}

impl AntNeuroClient {
    pub fn new(config: AntNeuroConfig) -> Self {
        Self { config }
    }

    /// Start streaming data. Returns a receiver for events and a handle for control.
    /// The streaming runs in a background tokio task until disconnected or an error occurs.
    pub fn start(&self) -> Result<(mpsc::Receiver<AntNeuroEvent>, AntNeuroHandle)> {
        let (tx, rx) = mpsc::channel::<AntNeuroEvent>(256);
        let config = self.config.clone();
        let paused = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));

        let handle = AntNeuroHandle {
            paused: Arc::clone(&paused),
            stop: Arc::clone(&stop),
        };

        tokio::spawn(async move {
            if let Err(e) = run_pipeline(config, tx.clone(), paused, stop).await {
                let _ = tx.send(AntNeuroEvent::Error(e.to_string())).await;
            }
            let _ = tx.send(AntNeuroEvent::Disconnected).await;
        });

        Ok((rx, handle))
    }
}

async fn run_pipeline(
    config: AntNeuroConfig,
    tx: mpsc::Sender<AntNeuroEvent>,
    paused: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
) -> Result<()> {
    let sdk = AntNeuroSdk::new(&config.library_path)?;

    let amp = sdk.open_first_amplifier()?;
    let info = sdk.get_amplifiers_info()?;
    if let Some(first) = info.first() {
        let _ = tx.send(AntNeuroEvent::Connected(first.clone())).await;
    }

    log::info!(
        "Amplifier: serial={}, type={}, firmware={}",
        amp.serial().unwrap_or_default(),
        amp.amplifier_type().unwrap_or_default(),
        amp.firmware_version().unwrap_or(-1),
    );

    let channels = amp.channel_list()?;
    let ref_range = config
        .reference_range
        .unwrap_or_else(|| amp.reference_ranges_available().unwrap_or_default()[0]);
    let bip_range = config
        .bipolar_range
        .unwrap_or_else(|| amp.bipolar_ranges_available().unwrap_or_default()[0]);

    if config.impedance_mode {
        let stream = amp.open_impedance_stream(&channels)?;
        let stream_channels = stream.channels().to_vec();
        loop {
            if tx.is_closed() || stop.load(Ordering::SeqCst) {
                break;
            }
            if paused.load(Ordering::SeqCst) {
                tokio::time::sleep(config.poll_interval).await;
                continue;
            }
            match stream.get_data()? {
                Some((ch_count, sample_count, samples)) => {
                    let now_ms = now_epoch_ms();
                    let _ = tx
                        .send(AntNeuroEvent::Impedance(ImpedanceData {
                            channel_count: ch_count,
                            sample_count,
                            samples,
                            timestamp_ms: now_ms,
                            channels: stream_channels.clone(),
                        }))
                        .await;
                }
                None => {
                    tokio::time::sleep(config.poll_interval).await;
                }
            }
        }
    } else {
        let stream =
            amp.open_eeg_stream(config.sampling_rate, ref_range, bip_range, &channels)?;
        let stream_channels = stream.channels().to_vec();
        loop {
            if tx.is_closed() || stop.load(Ordering::SeqCst) {
                break;
            }
            if paused.load(Ordering::SeqCst) {
                tokio::time::sleep(config.poll_interval).await;
                continue;
            }
            match stream.get_data()? {
                Some((ch_count, sample_count, samples)) => {
                    let now_ms = now_epoch_ms();
                    let _ = tx
                        .send(AntNeuroEvent::Eeg(EegData {
                            channel_count: ch_count,
                            sample_count,
                            samples,
                            timestamp_ms: now_ms,
                            channels: stream_channels.clone(),
                        }))
                        .await;
                }
                None => {
                    tokio::time::sleep(config.poll_interval).await;
                }
            }
        }
    }

    Ok(())
}

fn now_epoch_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
        * 1000.0
}

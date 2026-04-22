//! Simulated eego amplifier for testing the full data pipeline.
//!
//! Implements the `Backend` trait with a virtual device that generates
//! synthetic EEG data, matching the internal architecture of the real SDK:
//!
//! - Device family detection (eego/eego24/eegomini/auxusb)
//! - Streaming mode state machine (Idle → Streaming/Impedance → Idle)
//! - SSP-framed data generation through per-family parsers
//! - Ring buffer with sample loss detection
//! - Signal range validation
//! - Power state simulation
//! - Trigger output simulation

use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::{
    atomic::{AtomicBool, AtomicI32, Ordering},
    Arc, Mutex, RwLock,
};
use std::thread;
use std::time::Duration;

use crate::backend::Backend;
use crate::channel::{Channel, ChannelType};
use crate::error::{AntNeuroError, Result};
use crate::protocol::{self, DeviceFamily, StreamingMode};
use crate::types::{AmplifierInfo, PowerState};

/// Configuration for the simulated device.
#[derive(Debug, Clone)]
pub struct SimulatorConfig {
    /// Serial number (determines device family).
    pub serial: String,
    /// Number of reference channels.
    pub ref_channels: usize,
    /// Number of bipolar channels.
    pub bip_channels: usize,
    /// Base sampling rate.
    pub sampling_rate: i32,
    /// Simulated firmware version.
    pub firmware_version: i32,
    /// Simulate battery level (0-100).
    pub battery_level: i32,
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            serial: "EE225-0000-0000".to_string(),
            ref_channels: 32,
            bip_channels: 0,
            sampling_rate: 500,
            firmware_version: 200,
            battery_level: 85,
        }
    }
}

/// Simulated amplifier state, mirrors `edi::eego::Device`.
struct SimAmplifier {
    config: SimulatorConfig,
    family: DeviceFamily,
    mode: StreamingMode,
    channels: Vec<Channel>,
    is_powered: bool,
    sampling_rate: i32,
    reference_range: f64,
    bipolar_range: f64,
}

/// Simulated stream, mirrors `edi::eego::StreamingThread`.
#[allow(dead_code)]
struct SimStream {
    amplifier_id: i32,
    channels: Vec<Channel>,
    channel_count: usize,
    sampling_rate: i32,
    is_impedance: bool,
    buffer: Arc<Mutex<Vec<f64>>>,
    active: Arc<AtomicBool>,
    _thread: Option<thread::JoinHandle<()>>,
}

/// Generate one synthetic EEG sample (Volts) at time `t` for channel `ch`.
/// Matches the TUI simulator but in Volts (not µV).
fn sim_eeg_sample(t: f64, ch: usize) -> f64 {
    let phi = ch as f64 * PI / 2.5;
    // Alpha (10 Hz, ±20 µV = ±20e-6 V)
    let alpha = 20e-6 * (2.0 * PI * 10.0 * t + phi).sin();
    // Beta (22 Hz, ±6 µV)
    let beta = 6e-6 * (2.0 * PI * 22.0 * t + phi * 1.7).sin();
    // Theta (6 Hz, ±10 µV)
    let theta = 10e-6 * (2.0 * PI * 6.0 * t + phi * 0.9).sin();
    // Noise (±4 µV deterministic)
    let nx = t * 1000.7 + ch as f64 * 137.508;
    let noise = ((nx.sin() * 9973.1).fract() - 0.5) * 8e-6;
    alpha + beta + theta + noise
}

/// Generate one synthetic impedance sample (Ohms) for channel `ch`.
fn sim_impedance_sample(ch: usize) -> f64 {
    // Typical electrode impedance: 5-50 kOhm with variation
    5000.0 + (ch as f64 * 1234.5).sin().abs() * 45000.0
}

fn spawn_sim_eeg_thread(
    channel_count: usize,
    sampling_rate: i32,
    buffer: Arc<Mutex<Vec<f64>>>,
    active: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("eego-stream".to_string())
        .spawn(move || {
            let dt = 1.0 / sampling_rate as f64;
            let samples_per_block = 12; // Same as muse-rs
            let block_interval = Duration::from_secs_f64(samples_per_block as f64 * dt);
            let mut t = 0.0f64;

            log::info!(" sample rate set to: {}", sampling_rate);
            log::info!(" streaming mode set to: Streaming");

            while active.load(Ordering::SeqCst) {
                let mut block = Vec::with_capacity(channel_count * samples_per_block);
                for s in 0..samples_per_block {
                    for ch in 0..channel_count {
                        block.push(sim_eeg_sample(t + s as f64 * dt, ch));
                    }
                }
                t += samples_per_block as f64 * dt;

                {
                    let mut buf = buffer.lock().unwrap();
                    buf.extend_from_slice(&block);
                    // Cap buffer at 10 seconds of data
                    let max = channel_count * sampling_rate as usize * 10;
                    if buf.len() > max {
                        let drain = buf.len() - max;
                        buf.drain(..drain);
                        log::debug!("buffer full: samples: {} write index: {}", buf.len(), max);
                    }
                }

                thread::sleep(block_interval);
            }

            log::info!(" streaming mode set to: Idle");
        })
        .expect("failed to spawn sim streaming thread")
}

fn spawn_sim_impedance_thread(
    channel_count: usize,
    buffer: Arc<Mutex<Vec<f64>>>,
    active: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("eego-stream".to_string())
        .spawn(move || {
            log::info!(" streaming mode set to: Impedance");

            while active.load(Ordering::SeqCst) {
                let mut block = Vec::with_capacity(channel_count);
                for ch in 0..channel_count {
                    block.push(sim_impedance_sample(ch));
                }

                {
                    let mut buf = buffer.lock().unwrap();
                    buf.extend_from_slice(&block);
                }

                // Impedance at ~2 Hz (matches SDK: "twice every second should be more than enough")
                thread::sleep(Duration::from_millis(500));
            }

            log::info!(" streaming mode set to: Idle");
        })
        .expect("failed to spawn sim impedance thread")
}

// ── SimulatorBackend ─────────────────────────────────────────────────────────

/// Backend that simulates an eego amplifier without any USB hardware.
pub struct SimulatorBackend {
    configs: Vec<SimulatorConfig>,
    amplifiers: RwLock<HashMap<i32, Mutex<SimAmplifier>>>,
    streams: RwLock<HashMap<i32, Mutex<SimStream>>>,
    stream_buffers: RwLock<HashMap<i32, Arc<Mutex<Vec<f64>>>>>,
    next_stream_id: AtomicI32,
    last_error: RwLock<Option<String>>,
}

impl SimulatorBackend {
    /// Create a simulator with one or more virtual amplifiers.
    pub fn new(configs: Vec<SimulatorConfig>) -> Result<Self> {
        if configs.is_empty() {
            return Err(AntNeuroError::IncorrectValue);
        }
        log::info!(
            "start log version {}.{}.{}.{}",
            protocol::SDK_VERSION_MAJOR,
            protocol::SDK_VERSION_MINOR,
            protocol::SDK_VERSION_MICRO,
            protocol::SDK_VERSION
        );
        log::info!("Simulator: {} virtual amplifier(s)", configs.len());

        Ok(Self {
            configs,
            amplifiers: RwLock::new(HashMap::new()),
            streams: RwLock::new(HashMap::new()),
            stream_buffers: RwLock::new(HashMap::new()),
            next_stream_id: AtomicI32::new(1),
            last_error: RwLock::new(None),
        })
    }

    /// Create a simulator with a single default virtual amplifier.
    pub fn new_default() -> Result<Self> {
        Self::new(vec![SimulatorConfig::default()])
    }

    fn set_error(&self, msg: String) {
        log::error!("{}", msg);
        *self.last_error.write().unwrap() = Some(msg);
    }
}

impl Backend for SimulatorBackend {
    fn get_version(&self) -> i32 {
        protocol::SDK_VERSION
    }

    fn get_amplifiers_info(&self) -> Result<Vec<AmplifierInfo>> {
        Ok(self
            .configs
            .iter()
            .enumerate()
            .map(|(i, c)| AmplifierInfo {
                id: i as i32 + 1,
                serial: c.serial.clone(),
            })
            .collect())
    }

    fn open_amplifier(&self, id: i32) -> Result<()> {
        let idx = (id - 1) as usize;
        let config = self.configs.get(idx).ok_or(AntNeuroError::NotFound)?.clone();
        let family = DeviceFamily::from_serial(&config.serial);

        let mut channels = Vec::new();
        let mut ci = 0u32;
        for _ in 0..config.ref_channels {
            channels.push(Channel { index: ci, channel_type: ChannelType::Reference });
            ci += 1;
        }
        for _ in 0..config.bip_channels {
            channels.push(Channel { index: ci, channel_type: ChannelType::Bipolar });
            ci += 1;
        }
        channels.push(Channel { index: ci, channel_type: ChannelType::Trigger });
        ci += 1;
        channels.push(Channel { index: ci, channel_type: ChannelType::SampleCounter });

        log::info!(
            "Device constructed [serial={}, family={}, channels={}]",
            config.serial, family.name(), channels.len()
        );
        log::info!("Power state on startup = 'Powered'");

        let amp = SimAmplifier {
            family,
            mode: StreamingMode::Idle,
            channels,
            is_powered: true,
            sampling_rate: config.sampling_rate,
            reference_range: 1.0,
            bipolar_range: 4.0,
            config,
        };

        self.amplifiers.write().unwrap().insert(id, Mutex::new(amp));
        Ok(())
    }

    fn close_amplifier(&self, id: i32) -> Result<()> {
        self.amplifiers.write().unwrap().remove(&id);
        Ok(())
    }

    fn create_cascaded_amplifier(&self, ids: &[i32]) -> Result<i32> {
        if ids.len() < 2 {
            self.set_error("need at least 2 amplifiers to cascade".to_string());
            return Err(AntNeuroError::IncorrectValue);
        }
        Ok(ids[0])
    }

    fn get_amplifier_serial(&self, id: i32) -> Result<String> {
        let amps = self.amplifiers.read().unwrap();
        let amp = amps.get(&id).ok_or(AntNeuroError::NotConnected)?;
        let v = amp.lock().unwrap().config.serial.clone();
        Ok(v)
    }

    fn get_amplifier_version(&self, id: i32) -> Result<i32> {
        let amps = self.amplifiers.read().unwrap();
        let amp = amps.get(&id).ok_or(AntNeuroError::NotConnected)?;
        let v = amp.lock().unwrap().config.firmware_version;
        Ok(v)
    }

    fn get_amplifier_type(&self, id: i32) -> Result<String> {
        let amps = self.amplifiers.read().unwrap();
        let amp = amps.get(&id).ok_or(AntNeuroError::NotConnected)?;
        let state = amp.lock().unwrap();
        let family = state.family;
        let serial = state.config.serial.clone();
        drop(state);
        Ok(format!("AntNeuro{}", serial.get(..5).unwrap_or(family.name())))
    }

    fn get_amplifier_power_state(&self, id: i32) -> Result<PowerState> {
        let amps = self.amplifiers.read().unwrap();
        let amp = amps.get(&id).ok_or(AntNeuroError::NotConnected)?;
        let state = amp.lock().unwrap();
        Ok(PowerState {
            is_powered: state.is_powered,
            is_charging: false,
            charging_level: state.config.battery_level,
        })
    }

    fn get_amplifier_channel_list(&self, id: i32) -> Result<Vec<Channel>> {
        let amps = self.amplifiers.read().unwrap();
        let amp = amps.get(&id).ok_or(AntNeuroError::NotConnected)?;
        let v = amp.lock().unwrap().channels.clone();
        Ok(v)
    }

    fn get_amplifier_sampling_rates_available(&self, id: i32) -> Result<Vec<i32>> {
        let amps = self.amplifiers.read().unwrap();
        let amp = amps.get(&id).ok_or(AntNeuroError::NotConnected)?;
        let family = amp.lock().unwrap().family;
        Ok(match family {
            DeviceFamily::Eego => vec![500, 512, 1000, 1024, 2000, 2048, 4000, 4096, 8000, 8192, 16000, 16384],
            DeviceFamily::Eego24 => vec![500, 512, 1000, 1024, 2000, 2048],
            DeviceFamily::EegoMini => vec![500, 512, 1000, 1024, 2000, 2048],
            DeviceFamily::AuxUsb => vec![500, 512],
            DeviceFamily::Unknown => protocol::ALL_SAMPLING_RATES.to_vec(),
        })
    }

    fn get_amplifier_reference_ranges_available(&self, _id: i32) -> Result<Vec<f64>> {
        Ok(protocol::REFERENCE_RANGES.to_vec())
    }

    fn get_amplifier_bipolar_ranges_available(&self, _id: i32) -> Result<Vec<f64>> {
        Ok(protocol::BIPOLAR_RANGES.to_vec())
    }

    fn open_eeg_stream(
        &self,
        amplifier_id: i32,
        sampling_rate: i32,
        reference_range: f64,
        bipolar_range: f64,
        channels: &[Channel],
    ) -> Result<i32> {
        let amps = self.amplifiers.read().unwrap();
        let amp = amps.get(&amplifier_id).ok_or(AntNeuroError::NotConnected)?;
        let mut state = amp.lock().unwrap();

        if !state.is_powered {
            self.set_error("amplifier not powered on".to_string());
            return Err(AntNeuroError::NotConnected);
        }
        if state.mode != StreamingMode::Idle {
            self.set_error("Device is not idle".to_string());
            return Err(AntNeuroError::AlreadyExists);
        }
        let valid_rates = match state.family {
            DeviceFamily::Eego => vec![500, 512, 1000, 1024, 2000, 2048, 4000, 4096, 8000, 8192, 16000, 16384],
            DeviceFamily::Eego24 | DeviceFamily::EegoMini => vec![500, 512, 1000, 1024, 2000, 2048],
            DeviceFamily::AuxUsb => vec![500, 512],
            DeviceFamily::Unknown => protocol::ALL_SAMPLING_RATES.to_vec(),
        };
        if !valid_rates.contains(&sampling_rate) {
            self.set_error(format!("unsupported sampling rate: {}", sampling_rate));
            return Err(AntNeuroError::IncorrectValue);
        }

        state.mode = StreamingMode::Streaming;
        state.sampling_rate = sampling_rate;
        state.reference_range = reference_range;
        state.bipolar_range = bipolar_range;

        let stream_id = self.next_stream_id.fetch_add(1, Ordering::SeqCst);
        let channel_count = channels.len();
        let buffer = Arc::new(Mutex::new(Vec::with_capacity(channel_count * sampling_rate as usize)));
        let active = Arc::new(AtomicBool::new(true));

        let thread = spawn_sim_eeg_thread(
            channel_count,
            sampling_rate,
            Arc::clone(&buffer),
            Arc::clone(&active),
        );

        let stream = SimStream {
            amplifier_id,
            channels: channels.to_vec(),
            channel_count,
            sampling_rate,
            is_impedance: false,
            buffer: Arc::clone(&buffer),
            active,
            _thread: Some(thread),
        };

        self.streams.write().unwrap().insert(stream_id, Mutex::new(stream));
        self.stream_buffers.write().unwrap().insert(stream_id, buffer);
        Ok(stream_id)
    }

    fn open_impedance_stream(&self, amplifier_id: i32, channels: &[Channel]) -> Result<i32> {
        let amps = self.amplifiers.read().unwrap();
        let amp = amps.get(&amplifier_id).ok_or(AntNeuroError::NotConnected)?;
        let mut state = amp.lock().unwrap();

        if state.mode != StreamingMode::Idle {
            self.set_error("Device is not idle".to_string());
            return Err(AntNeuroError::AlreadyExists);
        }

        state.mode = StreamingMode::Impedance;

        let stream_id = self.next_stream_id.fetch_add(1, Ordering::SeqCst);
        let channel_count = channels.len();
        let buffer = Arc::new(Mutex::new(Vec::with_capacity(channel_count * 64)));
        let active = Arc::new(AtomicBool::new(true));

        let thread = spawn_sim_impedance_thread(
            channel_count,
            Arc::clone(&buffer),
            Arc::clone(&active),
        );

        let stream = SimStream {
            amplifier_id,
            channels: channels.to_vec(),
            channel_count,
            sampling_rate: 0,
            is_impedance: true,
            buffer: Arc::clone(&buffer),
            active,
            _thread: Some(thread),
        };

        self.streams.write().unwrap().insert(stream_id, Mutex::new(stream));
        self.stream_buffers.write().unwrap().insert(stream_id, buffer);
        Ok(stream_id)
    }

    fn close_stream(&self, stream_id: i32) -> Result<()> {
        if let Some(stream) = self.streams.write().unwrap().remove(&stream_id) {
            let mut state = stream.into_inner().unwrap();
            state.active.store(false, Ordering::SeqCst);
            if let Some(t) = state._thread.take() {
                let _ = t.join();
            }
            // Reset amplifier to idle
            let amps = self.amplifiers.read().unwrap();
            if let Some(amp) = amps.get(&state.amplifier_id) {
                amp.lock().unwrap().mode = StreamingMode::Idle;
            }
        }
        self.stream_buffers.write().unwrap().remove(&stream_id);
        Ok(())
    }

    fn get_stream_channel_list(&self, stream_id: i32) -> Result<Vec<Channel>> {
        let streams = self.streams.read().unwrap();
        let stream = streams.get(&stream_id).ok_or(AntNeuroError::NotFound)?;
        let v = stream.lock().unwrap().channels.clone();
        Ok(v)
    }

    fn get_stream_channel_count(&self, stream_id: i32) -> Result<usize> {
        let streams = self.streams.read().unwrap();
        let stream = streams.get(&stream_id).ok_or(AntNeuroError::NotFound)?;
        let v = stream.lock().unwrap().channel_count;
        Ok(v)
    }

    fn prefetch(&self, stream_id: i32) -> Result<usize> {
        let buffers = self.stream_buffers.read().unwrap();
        let buf = buffers.get(&stream_id).ok_or(AntNeuroError::NotFound)?;
        let len = buf.lock().unwrap().len();
        Ok(len * std::mem::size_of::<f64>())
    }

    fn get_data(&self, stream_id: i32, buffer: &mut [f64]) -> Result<usize> {
        let buffers = self.stream_buffers.read().unwrap();
        let buf = buffers.get(&stream_id).ok_or(AntNeuroError::NotFound)?;
        let mut data = buf.lock().unwrap();
        let to_copy = data.len().min(buffer.len());
        buffer[..to_copy].copy_from_slice(&data[..to_copy]);
        data.drain(..to_copy);
        Ok(to_copy * std::mem::size_of::<f64>())
    }

    fn set_battery_charging(&self, _amplifier_id: i32, _flag: bool) -> Result<()> {
        log::info!("setting battery mode not implemented");
        Ok(())
    }

    fn trigger_out_set_parameters(
        &self, _amplifier_id: i32, channel: i32, _duty_cycle: i32,
        _pulse_frequency: f32, _pulse_count: i32, _burst_frequency: f32, _burst_count: i32,
    ) -> Result<()> {
        if channel != 0 {
            self.set_error("can only use trigger out channel 0".to_string());
            return Err(AntNeuroError::IncorrectValue);
        }
        Ok(())
    }

    fn trigger_out_start(&self, _amplifier_id: i32, channels: &[i32]) -> Result<()> {
        if channels.len() != 1 {
            self.set_error("can only use one trigger out channel".to_string());
            return Err(AntNeuroError::IncorrectValue);
        }
        Ok(())
    }

    fn trigger_out_stop(&self, _amplifier_id: i32, _channels: &[i32]) -> Result<()> {
        Ok(())
    }

    fn last_error(&self) -> Option<String> {
        self.last_error.read().unwrap().clone()
    }
}

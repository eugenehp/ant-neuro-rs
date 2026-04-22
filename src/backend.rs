//! Backend trait abstracting the eego SDK operations.
//!
//! Two implementations:
//! - [`FfiBackend`](crate::ffi_backend::FfiBackend): loads the vendor `.so`/`.dll` via libloading
//! - [`NativeBackend`](crate::native::NativeBackend): pure-Rust USB via rusb
//!
//! # Sync vs Async
//!
//! The [`Backend`] trait is synchronous. For async usage, wrap any Backend in
//! [`AsyncBackend`] which runs each call on `tokio::task::spawn_blocking` and
//! provides a `tokio::sync::mpsc` channel for streaming data.

use crate::channel::Channel;
use crate::error::Result;
use crate::types::{AmplifierInfo, PowerState};

/// Configuration for trigger output.
///
/// Bundles the parameters for [`Backend::trigger_out_set_parameters`] into
/// a single struct for ergonomics.
#[derive(Debug, Clone)]
pub struct TriggerOutConfig {
    pub channel: i32,
    pub duty_cycle: i32,
    pub pulse_frequency: f32,
    pub pulse_count: i32,
    pub burst_frequency: f32,
    pub burst_count: i32,
}

/// Synchronous trait abstracting all eego SDK operations.
///
/// Both the FFI backend and native USB backend implement this trait with
/// identical semantics across 7 device families.
pub trait Backend: Send + Sync {
    // ── Lifecycle ───────────────────────────────────────────────────────────

    /// Returns the SDK version as a build number (e.g. 57168 = v1.3.29).
    fn get_version(&self) -> i32;

    // ── Amplifier discovery ─────────────────────────────────────────────────

    /// Enumerate all connected eego amplifiers.
    ///
    /// Returns a list of [`AmplifierInfo`] with unique `id` values (0-based).
    /// IDs are stable for the lifetime of the Backend instance.
    fn get_amplifiers_info(&self) -> Result<Vec<AmplifierInfo>>;

    /// Open an amplifier for streaming. Must be called before any per-amplifier
    /// operation. Returns [`NotConnected`](AntNeuroError::NotConnected) if the
    /// amplifier ID is invalid.
    fn open_amplifier(&self, amplifier_id: i32) -> Result<()>;

    /// Close an amplifier. Any active streams are stopped first.
    fn close_amplifier(&self, amplifier_id: i32) -> Result<()>;

    /// Create a cascaded (multi-device) amplifier from two or more open amplifiers.
    /// Returns the ID of the combined virtual amplifier.
    fn create_cascaded_amplifier(&self, amplifier_ids: &[i32]) -> Result<i32>;

    // ── Amplifier metadata ──────────────────────────────────────────────────
    //
    // All metadata methods require a prior `open_amplifier()` call.
    // Values are cached at open time and do not require USB transfers.

    /// Device serial number (trailing component of the full serial regex).
    fn get_amplifier_serial(&self, amplifier_id: i32) -> Result<String>;

    /// Firmware version parsed from the serial (middle regex group, as integer).
    fn get_amplifier_version(&self, amplifier_id: i32) -> Result<i32>;

    /// Device type string (first regex group, e.g. `"EE225"`).
    fn get_amplifier_type(&self, amplifier_id: i32) -> Result<String>;

    /// Battery and power state. Fields are updated asynchronously by the
    /// iso-stream parser; before streaming starts, defaults apply.
    fn get_amplifier_power_state(&self, amplifier_id: i32) -> Result<PowerState>;

    /// Channel list for this amplifier. Contains only Reference and Bipolar
    /// channels — Trigger and SampleCounter appear in stream-level lists only.
    fn get_amplifier_channel_list(&self, amplifier_id: i32) -> Result<Vec<Channel>>;

    /// Supported sampling rates (Hz), filtered by the device's max rate.
    fn get_amplifier_sampling_rates_available(&self, amplifier_id: i32) -> Result<Vec<i32>>;

    /// Supported reference voltage ranges (Volts).
    fn get_amplifier_reference_ranges_available(&self, amplifier_id: i32) -> Result<Vec<f64>>;

    /// Supported bipolar voltage ranges (Volts).
    fn get_amplifier_bipolar_ranges_available(&self, amplifier_id: i32) -> Result<Vec<f64>>;

    // ── Streams ─────────────────────────────────────────────────────────────

    /// Open an EEG data stream with the specified parameters.
    ///
    /// The amplifier must be idle (no other stream open). Pass the channel
    /// list from [`get_amplifier_channel_list`](Self::get_amplifier_channel_list).
    /// For devices with no bipolar channels, pass `bipolar_range = 0.0`.
    ///
    /// Returns a stream ID for use with `get_data`, `prefetch`, `close_stream`.
    fn open_eeg_stream(
        &self,
        amplifier_id: i32,
        sampling_rate: i32,
        reference_range: f64,
        bipolar_range: f64,
        channels: &[Channel],
    ) -> Result<i32>;

    /// Open an impedance measurement stream.
    fn open_impedance_stream(&self, amplifier_id: i32, channels: &[Channel]) -> Result<i32>;

    /// Close a stream and release resources. The streaming thread is stopped.
    fn close_stream(&self, stream_id: i32) -> Result<()>;

    /// Channel list for an open stream (includes Trigger + SampleCounter).
    fn get_stream_channel_list(&self, stream_id: i32) -> Result<Vec<Channel>>;

    /// Number of channels in the stream (including Trigger + SampleCounter).
    fn get_stream_channel_count(&self, stream_id: i32) -> Result<usize>;

    /// Hint the backend to prepare data. Returns the number of bytes available.
    fn prefetch(&self, stream_id: i32) -> Result<usize>;

    /// Read sample data into `buffer`. Returns the number of bytes written.
    /// Each sample is an `f64` in row-major order (sample-major, channel-minor).
    ///
    /// Returns [`IncorrectValue`](AntNeuroError::IncorrectValue) when no data
    /// is available (matches libeego's behavior).
    fn get_data(&self, stream_id: i32, buffer: &mut [f64]) -> Result<usize>;

    // ── Battery & trigger ───────────────────────────────────────────────────

    /// Enable or disable battery charging. Only supported on the eego (EE2xx)
    /// family; other families return [`InternalError`](AntNeuroError::InternalError).
    fn set_battery_charging(&self, amplifier_id: i32, flag: bool) -> Result<()>;

    /// Configure trigger output parameters. Only supported on eegomini (EE5xx).
    fn trigger_out_set_parameters(
        &self,
        amplifier_id: i32,
        channel: i32,
        duty_cycle: i32,
        pulse_frequency: f32,
        pulse_count: i32,
        burst_frequency: f32,
        burst_count: i32,
    ) -> Result<()>;

    /// Start trigger output on the specified channels. Only supported on eegomini.
    fn trigger_out_start(&self, amplifier_id: i32, channels: &[i32]) -> Result<()>;

    /// Stop trigger output on the specified channels. Only supported on eegomini.
    fn trigger_out_stop(&self, amplifier_id: i32, channels: &[i32]) -> Result<()>;

    // ── Error ───────────────────────────────────────────────────────────────

    /// Last error message from the SDK, if any. Cleared after successful calls.
    fn last_error(&self) -> Option<String>;
}

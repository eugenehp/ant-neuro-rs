//! Public data types for the antneuro SDK.

use crate::channel::Channel;

/// Information about a discovered amplifier.
///
/// Returned by [`Backend::get_amplifiers_info`](crate::backend::Backend::get_amplifiers_info).
/// The `id` is a 0-based index stable for the Backend's lifetime.
#[derive(Debug, Clone)]
pub struct AmplifierInfo {
    /// Amplifier identifier (0-based). Pass to `open_amplifier`, `get_amplifier_*`, etc.
    pub id: i32,
    /// Full serial string (e.g. `"EE225-00042-00000001"`).
    pub serial: String,
}

/// C-compatible amplifier info matching the `eemagine_sdk_amplifier_info` struct.
#[repr(C)]
#[derive(Debug, Clone)]
pub(crate) struct RawAmplifierInfo {
    pub id: i32,
    pub serial: [u8; 64],
}

impl From<&RawAmplifierInfo> for AmplifierInfo {
    fn from(raw: &RawAmplifierInfo) -> Self {
        let serial = raw
            .serial
            .iter()
            .take_while(|&&b| b != 0)
            .copied()
            .collect::<Vec<u8>>();
        AmplifierInfo {
            id: raw.id,
            serial: String::from_utf8_lossy(&serial).to_string(),
        }
    }
}

/// Battery and power state of the amplifier.
///
/// Fields are populated asynchronously by the iso-stream parser. Before
/// streaming starts, `is_powered` is true and other fields are at defaults.
#[derive(Debug, Clone)]
pub struct PowerState {
    /// Whether the amplifier is powered on.
    pub is_powered: bool,
    /// Whether the battery is currently charging.
    pub is_charging: bool,
    /// Battery level percentage (-1 = unknown/unavailable).
    pub charging_level: i32,
}

/// Parsed SDK version.
#[derive(Debug, Clone)]
pub struct SdkVersion {
    pub major: i32,
    pub minor: i32,
    pub micro: i32,
    pub build: i32,
}

/// A block of EEG sample data.
///
/// Samples are stored in row-major (sample-major, channel-minor) order:
/// `[s0_ch0, s0_ch1, …, s1_ch0, s1_ch1, …]`.
#[derive(Debug, Clone)]
pub struct EegData {
    /// Number of channels per sample row.
    pub channel_count: usize,
    /// Number of complete sample rows.
    pub sample_count: usize,
    /// Flat sample array. Length = `channel_count × sample_count`.
    pub samples: Vec<f64>,
    /// Timestamp of this block (milliseconds since Unix epoch).
    pub timestamp_ms: f64,
    /// Channel descriptors in the same order as sample columns.
    pub channels: Vec<Channel>,
}

impl EegData {
    /// Access one sample value by `(channel_index, sample_index)`.
    pub fn get(&self, channel: usize, sample: usize) -> f64 {
        self.samples[sample * self.channel_count + channel]
    }
}

/// A block of impedance measurement data.
///
/// Same layout as [`EegData`] but values are in Ohms.
#[derive(Debug, Clone)]
pub struct ImpedanceData {
    pub channel_count: usize,
    pub sample_count: usize,
    /// Impedance values in Ohms.
    pub samples: Vec<f64>,
    pub timestamp_ms: f64,
    pub channels: Vec<Channel>,
}

impl ImpedanceData {
    pub fn get(&self, channel: usize, sample: usize) -> f64 {
        self.samples[sample * self.channel_count + channel]
    }
}

/// Events emitted by [`AntNeuroClient`](crate::client::AntNeuroClient).
#[derive(Debug, Clone)]
pub enum AntNeuroEvent {
    /// Amplifier connected and ready.
    Connected(AmplifierInfo),
    /// New EEG data block.
    Eeg(EegData),
    /// New impedance data block.
    Impedance(ImpedanceData),
    /// Amplifier disconnected (clean or due to error).
    Disconnected,
    /// Non-fatal error message.
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::channel::{ChannelType, Channel};

    #[test]
    fn test_raw_amplifier_info_conversion() {
        let mut raw = RawAmplifierInfo {
            id: 7,
            serial: [0u8; 64],
        };
        let serial_bytes = b"EE225-00042-00000001";
        raw.serial[..serial_bytes.len()].copy_from_slice(serial_bytes);
        let info = AmplifierInfo::from(&raw);
        assert_eq!(info.id, 7);
        assert_eq!(info.serial, "EE225-00042-00000001");
    }

    #[test]
    fn test_raw_amplifier_info_empty_serial() {
        let raw = RawAmplifierInfo {
            id: 0,
            serial: [0u8; 64],
        };
        let info = AmplifierInfo::from(&raw);
        assert_eq!(info.id, 0);
        assert_eq!(info.serial, "");
    }

    #[test]
    fn test_eeg_data_get_indexing() {
        let data = EegData {
            channel_count: 3,
            sample_count: 2,
            samples: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            timestamp_ms: 0.0,
            channels: vec![],
        };
        // Row-major: [s0_ch0, s0_ch1, s0_ch2, s1_ch0, s1_ch1, s1_ch2]
        assert_eq!(data.get(0, 0), 1.0);
        assert_eq!(data.get(1, 0), 2.0);
        assert_eq!(data.get(2, 0), 3.0);
        assert_eq!(data.get(0, 1), 4.0);
        assert_eq!(data.get(1, 1), 5.0);
        assert_eq!(data.get(2, 1), 6.0);
    }

    #[test]
    fn test_impedance_data_get_indexing() {
        let data = ImpedanceData {
            channel_count: 2,
            sample_count: 3,
            samples: vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0],
            timestamp_ms: 0.0,
            channels: vec![],
        };
        assert_eq!(data.get(0, 0), 10.0);
        assert_eq!(data.get(1, 0), 20.0);
        assert_eq!(data.get(0, 1), 30.0);
        assert_eq!(data.get(1, 1), 40.0);
        assert_eq!(data.get(0, 2), 50.0);
        assert_eq!(data.get(1, 2), 60.0);
    }
}

//! Channel types and descriptors for eego amplifiers.

use std::fmt;

/// Electrode channel type, matching the eego SDK enumeration.
///
/// The integer values match the wire protocol encoding used in
/// `eemagine_sdk_channel_info` and SSP frame sample offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ChannelType {
    /// EEG reference electrode.
    Reference = 0,
    /// Bipolar (auxiliary) electrode.
    Bipolar = 1,
    /// Accelerometer axis (eegomini only).
    Accelerometer = 2,
    /// Gyroscope axis (eegomini only).
    Gyroscope = 3,
    /// Magnetometer axis (eegomini only).
    Magnetometer = 4,
    /// Digital trigger input/output.
    Trigger = 5,
    /// Monotonically increasing sample counter.
    SampleCounter = 6,
    /// Impedance on reference electrode (impedance mode only).
    ImpedanceReference = 7,
    /// Impedance on ground electrode (impedance mode only).
    ImpedanceGround = 8,
}

impl ChannelType {
    /// Convert a raw SDK integer to a channel type. Unknown values default
    /// to [`Reference`](ChannelType::Reference).
    pub fn from_raw(val: i32) -> Self {
        match val {
            0 => Self::Reference,
            1 => Self::Bipolar,
            2 => Self::Accelerometer,
            3 => Self::Gyroscope,
            4 => Self::Magnetometer,
            5 => Self::Trigger,
            6 => Self::SampleCounter,
            7 => Self::ImpedanceReference,
            8 => Self::ImpedanceGround,
            _ => Self::Reference,
        }
    }
}

impl fmt::Display for ChannelType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reference => write!(f, "REF"),
            Self::Bipolar => write!(f, "BIP"),
            Self::Accelerometer => write!(f, "ACC"),
            Self::Gyroscope => write!(f, "GYR"),
            Self::Magnetometer => write!(f, "MAG"),
            Self::Trigger => write!(f, "TRG"),
            Self::SampleCounter => write!(f, "CNT"),
            Self::ImpedanceReference => write!(f, "IMP_REF"),
            Self::ImpedanceGround => write!(f, "IMP_GND"),
        }
    }
}

/// A single channel descriptor.
///
/// Returned by [`Backend::get_amplifier_channel_list`](crate::backend::Backend::get_amplifier_channel_list)
/// and [`Backend::get_stream_channel_list`](crate::backend::Backend::get_stream_channel_list).
#[derive(Debug, Clone, Copy)]
pub struct Channel {
    /// 0-based channel index within the device.
    pub index: u32,
    /// Electrode type (Reference, Bipolar, Trigger, etc.).
    pub channel_type: ChannelType,
}

/// C-compatible channel info matching `eemagine_sdk_channel_info`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct RawChannelInfo {
    pub index: i32,
    pub channel_type: i32,
}

impl From<RawChannelInfo> for Channel {
    fn from(raw: RawChannelInfo) -> Self {
        Channel {
            index: raw.index as u32,
            channel_type: ChannelType::from_raw(raw.channel_type),
        }
    }
}

impl From<&Channel> for RawChannelInfo {
    fn from(ch: &Channel) -> Self {
        RawChannelInfo {
            index: ch.index as i32,
            channel_type: ch.channel_type as i32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_raw_all_valid() {
        assert_eq!(ChannelType::from_raw(0), ChannelType::Reference);
        assert_eq!(ChannelType::from_raw(1), ChannelType::Bipolar);
        assert_eq!(ChannelType::from_raw(2), ChannelType::Accelerometer);
        assert_eq!(ChannelType::from_raw(3), ChannelType::Gyroscope);
        assert_eq!(ChannelType::from_raw(4), ChannelType::Magnetometer);
        assert_eq!(ChannelType::from_raw(5), ChannelType::Trigger);
        assert_eq!(ChannelType::from_raw(6), ChannelType::SampleCounter);
        assert_eq!(ChannelType::from_raw(7), ChannelType::ImpedanceReference);
        assert_eq!(ChannelType::from_raw(8), ChannelType::ImpedanceGround);
    }

    #[test]
    fn test_from_raw_unknown_defaults_to_reference() {
        assert_eq!(ChannelType::from_raw(9), ChannelType::Reference);
        assert_eq!(ChannelType::from_raw(-1), ChannelType::Reference);
        assert_eq!(ChannelType::from_raw(100), ChannelType::Reference);
    }

    #[test]
    fn test_channel_display() {
        assert_eq!(format!("{}", ChannelType::Reference), "REF");
        assert_eq!(format!("{}", ChannelType::Bipolar), "BIP");
        assert_eq!(format!("{}", ChannelType::Accelerometer), "ACC");
        assert_eq!(format!("{}", ChannelType::Gyroscope), "GYR");
        assert_eq!(format!("{}", ChannelType::Magnetometer), "MAG");
        assert_eq!(format!("{}", ChannelType::Trigger), "TRG");
        assert_eq!(format!("{}", ChannelType::SampleCounter), "CNT");
        assert_eq!(format!("{}", ChannelType::ImpedanceReference), "IMP_REF");
        assert_eq!(format!("{}", ChannelType::ImpedanceGround), "IMP_GND");
    }

    #[test]
    fn test_raw_channel_info_round_trip() {
        let ch = Channel {
            index: 5,
            channel_type: ChannelType::Bipolar,
        };
        let raw = RawChannelInfo::from(&ch);
        assert_eq!(raw.index, 5);
        assert_eq!(raw.channel_type, 1); // Bipolar = 1
        let back: Channel = raw.into();
        assert_eq!(back.index, 5);
        assert_eq!(back.channel_type, ChannelType::Bipolar);
    }

    #[test]
    fn test_raw_channel_info_round_trip_all_types() {
        for raw_val in 0..=8 {
            let raw = RawChannelInfo {
                index: raw_val,
                channel_type: raw_val,
            };
            let ch: Channel = raw.into();
            assert_eq!(ch.index, raw_val as u32);
            let back = RawChannelInfo::from(&ch);
            assert_eq!(back.channel_type, raw_val);
        }
    }
}

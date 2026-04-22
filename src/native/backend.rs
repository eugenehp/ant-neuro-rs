// ── NativeBackend: top-level device manager ──────────────────────────────────

use std::collections::HashMap;
use std::sync::{
    atomic::AtomicI32,
    Arc, Mutex, RwLock,
};

use crate::channel::{Channel, ChannelType};
use crate::error::{AntNeuroError, Result};
use crate::protocol::{self, DeviceFamily, StreamingMode};
use crate::types::AmplifierInfo;

use super::discovery::{find_endpoints, select_alt_setting};
use super::state::{AmplifierState, StreamHandle};
use super::{lock, read_lock};

/// Pure-Rust backend that communicates with eego amplifiers over USB.
pub struct NativeBackend {
    pub(crate) amplifiers: RwLock<HashMap<i32, Mutex<AmplifierState>>>,
    pub(crate) streams: RwLock<HashMap<i32, StreamHandle>>,
    pub(crate) next_stream_id: AtomicI32,
    pub(crate) last_error: RwLock<Option<String>>,
}

impl NativeBackend {
    /// Create a new backend instance, initializing USB discovery.
    pub fn new() -> Result<Self> {
        if std::env::var(protocol::ENV_EEGO_DISABLE).is_ok() {
            log::info!("eego devices disabled via {}", protocol::ENV_EEGO_DISABLE);
        }

        log::info!(
            "start log version {}.{}.{}.{}",
            protocol::SDK_VERSION_MAJOR,
            protocol::SDK_VERSION_MINOR,
            protocol::SDK_VERSION_MICRO,
            protocol::SDK_VERSION
        );

        Ok(Self {
            amplifiers: RwLock::new(HashMap::new()),
            streams: RwLock::new(HashMap::new()),
            next_stream_id: AtomicI32::new(0),
            last_error: RwLock::new(None),
        })
    }

    /// Log and store an error message.
    pub(crate) fn set_error(&self, msg: String) {
        log::error!("{}", msg);
        if let Ok(mut e) = self.last_error.write() {
            *e = Some(msg);
        }
    }

    /// Open a USB device and enumerate its capabilities (channels, rates, ranges).
    pub(crate) fn open_and_enumerate(
        &self,
        usb_info: &crate::usb::UsbDeviceInfo,
    ) -> Result<AmplifierState> {
        let devices = rusb::devices().map_err(|e| {
            self.set_error(format!("init libusb: {}", e));
            AntNeuroError::InternalError
        })?;

        for device in devices.iter() {
            if device.bus_number() != usb_info.bus || device.address() != usb_info.address {
                continue;
            }

            let desc = device.device_descriptor().map_err(|_| AntNeuroError::InternalError)?;
            let usb_version = desc.usb_version().0 as u16;

            let handle = device.open().map_err(|e| {
                self.set_error(format!("could not attach libusb device: {}", e));
                AntNeuroError::NotConnected
            })?;

            #[cfg(target_os = "linux")]
            {
                if handle.kernel_driver_active(protocol::EEGO_INTERFACE).unwrap_or(false) {
                    log::debug!("could not detach kernel driver, attempting anyway");
                    let _ = handle.detach_kernel_driver(protocol::EEGO_INTERFACE);
                }
            }

            handle.claim_interface(protocol::EEGO_INTERFACE).map_err(|e| {
                self.set_error(format!("claim usb interface 0: {}", e));
                AntNeuroError::NotConnected
            })?;

            let endpoints = find_endpoints(&device);
            let family = DeviceFamily::from_serial(&usb_info.serial);

            log::info!(
                "Device constructed [serial={}, family={}, usb={}]",
                usb_info.serial,
                family.name(),
                if usb_version >= 0x0300 { "3.0" } else { "2.0" }
            );

            let iso_alt = select_alt_setting(&device, usb_version, 0);
            if iso_alt > 0 {
                let _ = handle.set_alternate_setting(protocol::EEGO_INTERFACE, iso_alt);
            }

            let handle = Arc::new(handle);

            // All capability metadata is derived from the serial number and
            // family at enumeration time -- no USB transfers needed here.
            let serial = usb_info.serial.clone();
            let device_type = Self::device_type_from_serial(&serial, family, &usb_info.product);
            let firmware_version = Self::parse_version_from_serial(&serial);
            let channels = Self::default_channel_list_for(family, &serial);
            let sampling_rates = Self::default_sampling_rates_for(family, &serial);
            let reference_ranges = protocol::REFERENCE_RANGES.to_vec();
            let bipolar_ranges = Self::default_bipolar_ranges_for(family);
            // Power state defaults: powered=true immediately after enumeration;
            // charging state is unknown until the streaming parser updates it.
            let is_powered = true;
            let is_charging = false;
            let charging_level: i32 = -1;

            return Ok(AmplifierState {
                handle,
                info: AmplifierInfo::from(usb_info),
                family,
                interface_num: protocol::EEGO_INTERFACE,
                ep_bulk_in: endpoints.bulk_in,
                ep_bulk_out: endpoints.bulk_out,
                ep_iso_in: endpoints.iso_in,
                iso_alt_setting: iso_alt,
                usb_version,
                channels,
                sampling_rates,
                reference_ranges,
                bipolar_ranges,
                current_mode: StreamingMode::Idle,
                is_powered,
                is_charging,
                charging_level,
                serial,
                device_type,
                firmware_version,
            });
        }

        Err(AntNeuroError::NotFound)
    }

    /// Parse firmware version from the serial number (middle hyphen-delimited segment).
    fn parse_version_from_serial(serial: &str) -> i32 {
        serial
            .split('-')
            .nth(1)
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(0)
    }

    /// Extract the public-facing serial (trailing segment after the last hyphen).
    pub(crate) fn parse_public_serial(serial: &str) -> String {
        serial
            .rsplit('-')
            .next()
            .map(|s| s.to_string())
            .unwrap_or_else(|| serial.to_string())
    }

    /// Parse the numeric model id from a serial like "EE225-00042-00000001".
    /// Returns `Some(225)` for `EE225-...`, `None` if the prefix doesn't parse.
    pub(crate) fn parse_model_num(serial: &str) -> Option<u32> {
        if !serial.starts_with("EE") || serial.len() < 5 {
            return None;
        }
        serial[2..5].parse().ok()
    }

    /// Build the default channel list for a given device family and model.
    fn default_channel_list_for(family: DeviceFamily, serial: &str) -> Vec<Channel> {
        let model = Self::parse_model_num(serial).unwrap_or(0);
        let (ref_count, bip_count) = match family {
            DeviceFamily::Eego => {
                if serial.starts_with("EE301") {
                    // EE301 has no channel list.
                    (0, 0)
                } else if model == 213 || model == 221 {
                    // 32-channel variant (no bipolar channels).
                    (32, 0)
                } else {
                    // Standard eego: 64 reference + 24 bipolar.
                    (64, 24)
                }
            }
            DeviceFamily::Eego24 => (24, 0),
            DeviceFamily::EegoMini => (24, 4),
            DeviceFamily::AuxUsb => (0, 0),
            DeviceFamily::Unknown => (0, 0),
        };

        let mut channels = Vec::new();
        let mut idx = 0u32;
        for _ in 0..ref_count {
            channels.push(Channel { index: idx, channel_type: ChannelType::Reference });
            idx += 1;
        }
        for _ in 0..bip_count {
            channels.push(Channel { index: idx, channel_type: ChannelType::Bipolar });
            idx += 1;
        }
        // Trigger and SampleCounter channels are added per-stream, not at
        // the amplifier level.
        channels
    }

    /// Build the default sampling rate list, filtered by the family's maximum rate.
    fn default_sampling_rates_for(family: DeviceFamily, serial: &str) -> Vec<i32> {
        let model = Self::parse_model_num(serial).unwrap_or(0);
        let max_rate: i32 = match family {
            DeviceFamily::Eego => {
                if serial.starts_with("EE301") {
                    2000
                } else if (201..=215).contains(&model) || model == 213 || model == 221 {
                    2048
                } else {
                    16384
                }
            }
            DeviceFamily::Eego24 => 8192,
            DeviceFamily::EegoMini => 4096,
            DeviceFamily::AuxUsb => 2000,
            DeviceFamily::Unknown => 16384,
        };
        const ALL: &[i32] = &[
            500, 512, 1000, 1024, 2000, 2048,
            4000, 4096, 8000, 8192, 16000, 16384,
        ];
        ALL.iter().copied().filter(|&r| r <= max_rate).collect()
    }

    /// Per-family bipolar range list. The eegomini uses a different set.
    fn default_bipolar_ranges_for(family: DeviceFamily) -> Vec<f64> {
        match family {
            DeviceFamily::EegoMini => vec![2.5, 1.875, 0.375],
            _ => protocol::BIPOLAR_RANGES.to_vec(),
        }
    }

    /// Derive the device-type string from the serial number prefix (e.g. "EE225").
    fn device_type_from_serial(
        serial: &str,
        family: DeviceFamily,
        product_fallback: &str,
    ) -> String {
        if let Some(first) = serial.split('-').next() {
            if !first.is_empty() {
                return first.to_string();
            }
        }
        match family {
            DeviceFamily::Eego => "EE2xx".to_string(),
            DeviceFamily::Eego24 => "EE4xx".to_string(),
            DeviceFamily::EegoMini => "EE5xx".to_string(),
            DeviceFamily::AuxUsb => "AuxUSB".to_string(),
            DeviceFamily::Unknown => product_fallback.to_string(),
        }
    }

    /// Check that the amplifier belongs to the eegomini family (the only
    /// family that supports trigger output).
    pub(crate) fn trigger_out_family_check(&self, amplifier_id: i32) -> Result<()> {
        let amps = read_lock(&self.amplifiers)?;
        let amp = amps.get(&amplifier_id).ok_or(AntNeuroError::NotConnected)?;
        let state = lock(amp)?;
        if state.family == DeviceFamily::EegoMini && !state.channels.is_empty() {
            return Ok(());
        }
        Err(AntNeuroError::InternalError)
    }

    /// Reject operations that the eego24 family does not support.
    ///
    /// The vendor SDK's eego24 command class is incomplete: most operations
    /// beyond `get_amplifier_type` return an internal error.
    pub(crate) fn reject_if_eego24(&self, amplifier_id: i32) -> Result<()> {
        let amps = read_lock(&self.amplifiers)?;
        let amp = amps.get(&amplifier_id).ok_or(AntNeuroError::NotConnected)?;
        let state = lock(amp)?;
        if state.family == DeviceFamily::Eego24 {
            return Err(AntNeuroError::InternalError);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_from_serial() {
        assert_eq!(NativeBackend::parse_version_from_serial("EE225-00042-00000001"), 42);
        assert_eq!(NativeBackend::parse_version_from_serial("EE225-00000-00000001"), 0);
        assert_eq!(NativeBackend::parse_version_from_serial("nohyphen"), 0);
        assert_eq!(NativeBackend::parse_version_from_serial("EE225-abc-00000001"), 0);
    }

    #[test]
    fn test_parse_public_serial() {
        assert_eq!(NativeBackend::parse_public_serial("EE225-00042-00000001"), "00000001");
        assert_eq!(NativeBackend::parse_public_serial("nohyphen"), "nohyphen");
        assert_eq!(NativeBackend::parse_public_serial("a-b-c"), "c");
    }

    #[test]
    fn test_parse_model_num() {
        assert_eq!(NativeBackend::parse_model_num("EE225-00042-00000001"), Some(225));
        assert_eq!(NativeBackend::parse_model_num("EE410-00001"), Some(410));
        assert_eq!(NativeBackend::parse_model_num("EE520-00001"), Some(520));
        assert_eq!(NativeBackend::parse_model_num("M0001-00001"), None); // no EE prefix
        assert_eq!(NativeBackend::parse_model_num("EEabc-00001"), None);
        assert_eq!(NativeBackend::parse_model_num("XX"), None);
    }

    #[test]
    fn test_default_channel_list_eego_standard() {
        let channels = NativeBackend::default_channel_list_for(DeviceFamily::Eego, "EE225-00042-00000001");
        let ref_count = channels.iter().filter(|c| c.channel_type == ChannelType::Reference).count();
        let bip_count = channels.iter().filter(|c| c.channel_type == ChannelType::Bipolar).count();
        assert_eq!(ref_count, 64);
        assert_eq!(bip_count, 24);
    }

    #[test]
    fn test_default_channel_list_eego_ee301() {
        let channels = NativeBackend::default_channel_list_for(DeviceFamily::Eego, "EE301-00001-00000001");
        assert_eq!(channels.len(), 0);
    }

    #[test]
    fn test_default_channel_list_eego_32ch() {
        let channels = NativeBackend::default_channel_list_for(DeviceFamily::Eego, "EE213-00001-00000001");
        let ref_count = channels.iter().filter(|c| c.channel_type == ChannelType::Reference).count();
        let bip_count = channels.iter().filter(|c| c.channel_type == ChannelType::Bipolar).count();
        assert_eq!(ref_count, 32);
        assert_eq!(bip_count, 0);
    }

    #[test]
    fn test_default_channel_list_eego24() {
        let channels = NativeBackend::default_channel_list_for(DeviceFamily::Eego24, "EE410-00001-00000001");
        assert_eq!(channels.len(), 24);
        assert!(channels.iter().all(|c| c.channel_type == ChannelType::Reference));
    }

    #[test]
    fn test_default_channel_list_eegomini() {
        let channels = NativeBackend::default_channel_list_for(DeviceFamily::EegoMini, "EE520-00001-00000001");
        let ref_count = channels.iter().filter(|c| c.channel_type == ChannelType::Reference).count();
        let bip_count = channels.iter().filter(|c| c.channel_type == ChannelType::Bipolar).count();
        assert_eq!(ref_count, 24);
        assert_eq!(bip_count, 4);
    }

    #[test]
    fn test_default_channel_list_auxusb() {
        let channels = NativeBackend::default_channel_list_for(DeviceFamily::AuxUsb, "ZZ001-00001-00000001");
        assert_eq!(channels.len(), 0);
    }

    #[test]
    fn test_default_sampling_rates_eego_standard() {
        let rates = NativeBackend::default_sampling_rates_for(DeviceFamily::Eego, "EE225-00042-00000001");
        assert!(rates.contains(&500));
        assert!(rates.contains(&16384));
    }

    #[test]
    fn test_default_sampling_rates_eego_ee301() {
        let rates = NativeBackend::default_sampling_rates_for(DeviceFamily::Eego, "EE301-00001-00000001");
        assert!(rates.contains(&500));
        assert!(rates.contains(&2000));
        assert!(!rates.contains(&2048));
    }

    #[test]
    fn test_default_sampling_rates_eego24() {
        let rates = NativeBackend::default_sampling_rates_for(DeviceFamily::Eego24, "EE410-00001-00000001");
        assert!(rates.contains(&8192));
        assert!(!rates.contains(&16000));
    }

    #[test]
    fn test_default_sampling_rates_eegomini() {
        let rates = NativeBackend::default_sampling_rates_for(DeviceFamily::EegoMini, "EE520-00001-00000001");
        assert!(rates.contains(&4096));
        assert!(!rates.contains(&8000));
    }

    #[test]
    fn test_default_bipolar_ranges_eegomini() {
        let ranges = NativeBackend::default_bipolar_ranges_for(DeviceFamily::EegoMini);
        assert_eq!(ranges, vec![2.5, 1.875, 0.375]);
    }

    #[test]
    fn test_default_bipolar_ranges_eego() {
        let ranges = NativeBackend::default_bipolar_ranges_for(DeviceFamily::Eego);
        assert_eq!(ranges, vec![4.0, 1.5, 0.7, 0.35]);
    }
}

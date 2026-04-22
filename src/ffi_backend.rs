//! FFI backend: wraps the vendor's native .so/.dll via SdkBindings.

use crate::backend::Backend;
use crate::channel::{Channel, RawChannelInfo};
use crate::error::{check_rc, Result};
use crate::ffi::SdkBindings;
use crate::types::{AmplifierInfo, PowerState, RawAmplifierInfo};
use std::path::Path;

pub struct FfiBackend {
    sdk: SdkBindings,
}

impl FfiBackend {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            sdk: SdkBindings::load(path)?,
        })
    }
}

impl Backend for FfiBackend {
    fn get_version(&self) -> i32 {
        unsafe { (self.sdk.get_version)() }
    }

    fn get_amplifiers_info(&self) -> Result<Vec<AmplifierInfo>> {
        let count =
            check_rc(unsafe { (self.sdk.get_amplifiers_info)(std::ptr::null_mut(), 0) })?;
        if count == 0 {
            return Ok(Vec::new());
        }
        let mut raw = vec![
            RawAmplifierInfo {
                id: 0,
                serial: [0u8; 64],
            };
            count as usize
        ];
        check_rc(unsafe { (self.sdk.get_amplifiers_info)(raw.as_mut_ptr(), count) })?;
        Ok(raw.iter().map(AmplifierInfo::from).collect())
    }

    fn open_amplifier(&self, id: i32) -> Result<()> {
        check_rc(unsafe { (self.sdk.open_amplifier)(id) })?;
        Ok(())
    }

    fn close_amplifier(&self, id: i32) -> Result<()> {
        check_rc(unsafe { (self.sdk.close_amplifier)(id) })?;
        Ok(())
    }

    fn create_cascaded_amplifier(&self, ids: &[i32]) -> Result<i32> {
        check_rc(unsafe {
            (self.sdk.create_cascaded_amplifier)(ids.as_ptr(), ids.len() as i32)
        })
    }

    fn get_amplifier_serial(&self, id: i32) -> Result<String> {
        let mut buf = [0u8; 64];
        check_rc(unsafe {
            (self.sdk.get_amplifier_serial)(id, buf.as_mut_ptr(), buf.len() as i32)
        })?;
        Ok(String::from_utf8_lossy(&buf)
            .trim_end_matches('\0')
            .to_string())
    }

    fn get_amplifier_version(&self, id: i32) -> Result<i32> {
        check_rc(unsafe { (self.sdk.get_amplifier_version)(id) })
    }

    fn get_amplifier_type(&self, id: i32) -> Result<String> {
        let mut buf = [0u8; 64];
        check_rc(unsafe {
            (self.sdk.get_amplifier_type)(id, buf.as_mut_ptr(), buf.len() as i32)
        })?;
        Ok(String::from_utf8_lossy(&buf)
            .trim_end_matches('\0')
            .to_string())
    }

    fn get_amplifier_power_state(&self, id: i32) -> Result<PowerState> {
        let mut is_powered = 0i32;
        let mut is_charging = 0i32;
        let mut charging_level = 0i32;
        check_rc(unsafe {
            (self.sdk.get_amplifier_power_state)(
                id,
                &mut is_powered,
                &mut is_charging,
                &mut charging_level,
            )
        })?;
        Ok(PowerState {
            is_powered: is_powered != 0,
            is_charging: is_charging != 0,
            charging_level,
        })
    }

    fn get_amplifier_channel_list(&self, id: i32) -> Result<Vec<Channel>> {
        // The SDK C API does NOT support a null-buffer "probe" call — the
        // implementation at FUN_0013d310 enters its copy loop immediately
        // and throws `getAmplifierChannelList info array not large enough`
        // when capacity=0. Pre-allocate a generous upper bound (larger than
        // the biggest supported eego amp) and trim by the returned count.
        const MAX_CHANNELS: usize = 256;
        let mut raw = vec![
            RawChannelInfo { index: 0, channel_type: 0 };
            MAX_CHANNELS
        ];
        let count = check_rc(unsafe {
            (self.sdk.get_amplifier_channel_list)(id, raw.as_mut_ptr(), MAX_CHANNELS as i32)
        })? as usize;
        raw.truncate(count);
        Ok(raw.into_iter().map(Channel::from).collect())
    }

    fn get_amplifier_sampling_rates_available(&self, id: i32) -> Result<Vec<i32>> {
        // See get_amplifier_channel_list — no null-probe pattern.
        const MAX_RATES: usize = 32;
        let mut rates = vec![0i32; MAX_RATES];
        let count = check_rc(unsafe {
            (self.sdk.get_amplifier_sampling_rates_available)(
                id,
                rates.as_mut_ptr(),
                MAX_RATES as i32,
            )
        })? as usize;
        rates.truncate(count);
        Ok(rates)
    }

    fn get_amplifier_reference_ranges_available(&self, id: i32) -> Result<Vec<f64>> {
        const MAX_RANGES: usize = 16;
        let mut ranges = vec![0.0f64; MAX_RANGES];
        let count = check_rc(unsafe {
            (self.sdk.get_amplifier_reference_ranges_available)(
                id,
                ranges.as_mut_ptr(),
                MAX_RANGES as i32,
            )
        })? as usize;
        ranges.truncate(count);
        Ok(ranges)
    }

    fn get_amplifier_bipolar_ranges_available(&self, id: i32) -> Result<Vec<f64>> {
        const MAX_RANGES: usize = 16;
        let mut ranges = vec![0.0f64; MAX_RANGES];
        let count = check_rc(unsafe {
            (self.sdk.get_amplifier_bipolar_ranges_available)(
                id,
                ranges.as_mut_ptr(),
                MAX_RANGES as i32,
            )
        })? as usize;
        ranges.truncate(count);
        Ok(ranges)
    }

    fn open_eeg_stream(
        &self,
        amplifier_id: i32,
        sampling_rate: i32,
        reference_range: f64,
        bipolar_range: f64,
        channels: &[Channel],
    ) -> Result<i32> {
        let raw: Vec<RawChannelInfo> = channels.iter().map(RawChannelInfo::from).collect();
        check_rc(unsafe {
            (self.sdk.open_eeg_stream)(
                amplifier_id,
                sampling_rate,
                reference_range,
                bipolar_range,
                raw.as_ptr(),
                raw.len() as i32,
            )
        })
    }

    fn open_impedance_stream(&self, amplifier_id: i32, channels: &[Channel]) -> Result<i32> {
        let raw: Vec<RawChannelInfo> = channels.iter().map(RawChannelInfo::from).collect();
        check_rc(unsafe {
            (self.sdk.open_impedance_stream)(amplifier_id, raw.as_ptr(), raw.len() as i32)
        })
    }

    fn close_stream(&self, stream_id: i32) -> Result<()> {
        check_rc(unsafe { (self.sdk.close_stream)(stream_id) })?;
        Ok(())
    }

    fn get_stream_channel_list(&self, stream_id: i32) -> Result<Vec<Channel>> {
        let count = check_rc(unsafe { (self.sdk.get_stream_channel_count)(stream_id) })? as usize;
        let mut raw = vec![
            RawChannelInfo {
                index: 0,
                channel_type: 0,
            };
            count
        ];
        check_rc(unsafe {
            (self.sdk.get_stream_channel_list)(stream_id, raw.as_mut_ptr(), count as i32)
        })?;
        Ok(raw.into_iter().map(Channel::from).collect())
    }

    fn get_stream_channel_count(&self, stream_id: i32) -> Result<usize> {
        Ok(check_rc(unsafe { (self.sdk.get_stream_channel_count)(stream_id) })? as usize)
    }

    fn prefetch(&self, stream_id: i32) -> Result<usize> {
        Ok(check_rc(unsafe { (self.sdk.prefetch)(stream_id) })? as usize)
    }

    fn get_data(&self, stream_id: i32, buffer: &mut [f64]) -> Result<usize> {
        let bytes = buffer.len() * std::mem::size_of::<f64>();
        Ok(check_rc(unsafe {
            (self.sdk.get_data)(stream_id, buffer.as_mut_ptr(), bytes as i32)
        })? as usize)
    }

    fn set_battery_charging(&self, amplifier_id: i32, flag: bool) -> Result<()> {
        check_rc(unsafe {
            (self.sdk.set_battery_charging)(amplifier_id, if flag { 1 } else { 0 })
        })?;
        Ok(())
    }

    fn trigger_out_set_parameters(
        &self,
        amplifier_id: i32,
        channel: i32,
        duty_cycle: i32,
        pulse_frequency: f32,
        pulse_count: i32,
        burst_frequency: f32,
        burst_count: i32,
    ) -> Result<()> {
        check_rc(unsafe {
            (self.sdk.trigger_out_set_parameters)(
                amplifier_id,
                channel,
                duty_cycle,
                pulse_frequency,
                pulse_count,
                burst_frequency,
                burst_count,
            )
        })?;
        Ok(())
    }

    fn trigger_out_start(&self, amplifier_id: i32, channels: &[i32]) -> Result<()> {
        check_rc(unsafe {
            (self.sdk.trigger_out_start)(amplifier_id, channels.as_ptr(), channels.len() as i32)
        })?;
        Ok(())
    }

    fn trigger_out_stop(&self, amplifier_id: i32, channels: &[i32]) -> Result<()> {
        check_rc(unsafe {
            (self.sdk.trigger_out_stop)(amplifier_id, channels.as_ptr(), channels.len() as i32)
        })?;
        Ok(())
    }

    fn last_error(&self) -> Option<String> {
        self.sdk.last_error()
    }
}

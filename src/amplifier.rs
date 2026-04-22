use std::sync::Arc;

use crate::backend::Backend;
use crate::channel::Channel;
use crate::error::Result;
use crate::stream::Stream;
use crate::types::PowerState;

/// A connected ANT Neuro amplifier.
pub struct Amplifier {
    pub(crate) backend: Arc<dyn Backend>,
    pub(crate) id: i32,
}

impl Amplifier {
    /// Get the amplifier's serial number.
    pub fn serial(&self) -> Result<String> {
        self.backend.get_amplifier_serial(self.id)
    }

    /// Get the firmware version number.
    pub fn firmware_version(&self) -> Result<i32> {
        self.backend.get_amplifier_version(self.id)
    }

    /// Get the amplifier type string.
    pub fn amplifier_type(&self) -> Result<String> {
        self.backend.get_amplifier_type(self.id)
    }

    /// Get the power/battery state.
    pub fn power_state(&self) -> Result<PowerState> {
        self.backend.get_amplifier_power_state(self.id)
    }

    /// Get all channels available on this amplifier.
    pub fn channel_list(&self) -> Result<Vec<Channel>> {
        self.backend.get_amplifier_channel_list(self.id)
    }

    /// Get available sampling rates.
    pub fn sampling_rates_available(&self) -> Result<Vec<i32>> {
        self.backend.get_amplifier_sampling_rates_available(self.id)
    }

    /// Get available reference channel voltage ranges.
    pub fn reference_ranges_available(&self) -> Result<Vec<f64>> {
        self.backend
            .get_amplifier_reference_ranges_available(self.id)
    }

    /// Get available bipolar channel voltage ranges.
    pub fn bipolar_ranges_available(&self) -> Result<Vec<f64>> {
        self.backend
            .get_amplifier_bipolar_ranges_available(self.id)
    }

    /// Open an EEG data stream with specified parameters.
    /// Data values are in Volts. Only one stream can be active per amplifier.
    pub fn open_eeg_stream(
        &self,
        sampling_rate: i32,
        reference_range: f64,
        bipolar_range: f64,
        channels: &[Channel],
    ) -> Result<Stream> {
        let stream_id = self.backend.open_eeg_stream(
            self.id,
            sampling_rate,
            reference_range,
            bipolar_range,
            channels,
        )?;
        let channel_count = self.backend.get_stream_channel_count(stream_id)?;
        let stream_channels = self.backend.get_stream_channel_list(stream_id)?;
        Ok(Stream {
            backend: Arc::clone(&self.backend),
            stream_id,
            channel_count,
            channels: stream_channels,
        })
    }

    /// Open an EEG stream with default ranges and all channels.
    pub fn open_eeg_stream_default(&self, sampling_rate: i32) -> Result<Stream> {
        let channels = self.channel_list()?;
        let ref_range = self.reference_ranges_available()?[0];
        let bip_range = self.bipolar_ranges_available()?[0];
        self.open_eeg_stream(sampling_rate, ref_range, bip_range, &channels)
    }

    /// Open an impedance measurement stream.
    /// Data values are in Ohms.
    pub fn open_impedance_stream(&self, channels: &[Channel]) -> Result<Stream> {
        let stream_id = self.backend.open_impedance_stream(self.id, channels)?;
        let channel_count = self.backend.get_stream_channel_count(stream_id)?;
        let stream_channels = self.backend.get_stream_channel_list(stream_id)?;
        Ok(Stream {
            backend: Arc::clone(&self.backend),
            stream_id,
            channel_count,
            channels: stream_channels,
        })
    }

    /// Set battery charging on/off.
    pub fn set_battery_charging(&self, charging: bool) -> Result<()> {
        self.backend.set_battery_charging(self.id, charging)
    }

    /// Configure trigger output parameters for a channel.
    pub fn set_trigger_out_parameters(
        &self,
        channel: i32,
        duty_cycle: i32,
        pulse_frequency: f32,
        pulse_count: i32,
        burst_frequency: f32,
        burst_count: i32,
    ) -> Result<()> {
        self.backend.trigger_out_set_parameters(
            self.id,
            channel,
            duty_cycle,
            pulse_frequency,
            pulse_count,
            burst_frequency,
            burst_count,
        )
    }

    /// Start trigger output on specified channels.
    pub fn start_trigger_out(&self, channels: &[i32]) -> Result<()> {
        self.backend.trigger_out_start(self.id, channels)
    }

    /// Stop trigger output on specified channels.
    pub fn stop_trigger_out(&self, channels: &[i32]) -> Result<()> {
        self.backend.trigger_out_stop(self.id, channels)
    }
}

impl Drop for Amplifier {
    fn drop(&mut self) {
        if self.id >= 0 {
            let _ = self.backend.close_amplifier(self.id);
        }
    }
}

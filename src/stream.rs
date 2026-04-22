use std::sync::Arc;

use crate::backend::Backend;
use crate::channel::Channel;
use crate::error::Result;

/// An active data stream from an amplifier (EEG or impedance).
pub struct Stream {
    pub(crate) backend: Arc<dyn Backend>,
    pub(crate) stream_id: i32,
    pub(crate) channel_count: usize,
    pub(crate) channels: Vec<Channel>,
}

impl Stream {
    /// Get the channels in this stream.
    pub fn channels(&self) -> &[Channel] {
        &self.channels
    }

    /// Get the number of channels.
    pub fn channel_count(&self) -> usize {
        self.channel_count
    }

    /// Check how many bytes the next `get_data` call will need.
    /// Returns 0 if no data is ready yet.
    pub fn prefetch(&self) -> Result<usize> {
        self.backend.prefetch(self.stream_id)
    }

    /// Read the next block of samples.
    /// Returns `(channel_count, sample_count, samples_flat)` where samples
    /// are in row-major order: `[s0_ch0, s0_ch1, ..., s1_ch0, ...]`.
    /// Values are in Volts for EEG streams, Ohms for impedance streams.
    pub fn get_data(&self) -> Result<Option<(usize, usize, Vec<f64>)>> {
        let bytes_needed = self.prefetch()?;
        if bytes_needed == 0 {
            return Ok(None);
        }

        let num_doubles = bytes_needed / std::mem::size_of::<f64>();
        let mut buffer = vec![0.0f64; num_doubles];

        self.backend.get_data(self.stream_id, &mut buffer)?;

        let sample_count = num_doubles / self.channel_count;
        Ok(Some((self.channel_count, sample_count, buffer)))
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        let _ = self.backend.close_stream(self.stream_id);
    }
}

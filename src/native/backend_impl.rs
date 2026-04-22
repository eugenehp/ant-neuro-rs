// ── Backend trait implementation for NativeBackend ───────────────────────────

use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};

use crate::backend::Backend;
use crate::channel::{Channel, ChannelType};
use crate::error::{AntNeuroError, Result};
use crate::protocol::{self, DeviceFamily, StreamingMode};
use crate::types::{AmplifierInfo, PowerState};

use super::ring_buffer::RingBuffer;
use super::ssp_parser::{create_parser, DataParser};
use super::state::{StreamHandle, StreamState};
use super::streaming::spawn_streaming_thread;
use super::usb_command::{range_lut_code, UsbCommand};
use super::{lock, read_lock, write_lock, RING_BUF_SECONDS};

use super::backend::NativeBackend;

impl Backend for NativeBackend {
    fn get_version(&self) -> i32 {
        protocol::SDK_VERSION
    }

    fn get_amplifiers_info(&self) -> Result<Vec<AmplifierInfo>> {
        if std::env::var(protocol::ENV_EEGO_DISABLE).is_ok() {
            return Ok(Vec::new());
        }
        let usb_devices = crate::usb::find_eego_devices()?;
        Ok(usb_devices
            .iter()
            .enumerate()
            .map(|(idx, d)| {
                let mut info = AmplifierInfo::from(d);
                info.id = idx as i32;
                info
            })
            .collect())
    }

    fn open_amplifier(&self, id: i32) -> Result<()> {
        let usb_devices = crate::usb::find_eego_devices()?;
        let usb_info = usb_devices
            .get(id as usize)
            .ok_or_else(|| {
                self.set_error(format!("could not instantiate device for serial: id={}", id));
                AntNeuroError::NotFound
            })?
            .clone();

        let state = self.open_and_enumerate(&usb_info)?;
        write_lock(&self.amplifiers)?.insert(id, Mutex::new(state));
        Ok(())
    }

    fn close_amplifier(&self, id: i32) -> Result<()> {
        if let Some(amp) = write_lock(&self.amplifiers)?.remove(&id) {
            let Ok(state) = amp.into_inner() else {
                log::error!("close_amplifier: lock poisoned");
                return Err(AntNeuroError::InternalError);
            };
            let _ = UsbCommand::set_streaming_mode(&state.handle, StreamingMode::Idle);
            let _ = state.handle.release_interface(state.interface_num);
            log::info!("Detached [serial={}]", state.serial);
        }
        Ok(())
    }

    fn create_cascaded_amplifier(&self, ids: &[i32]) -> Result<i32> {
        if ids.len() < 2 {
            self.set_error("need at least 2 amplifiers to cascade".to_string());
            return Err(AntNeuroError::IncorrectValue);
        }
        let amps = read_lock(&self.amplifiers)?;
        let mut modes = Vec::new();
        for &id in ids {
            let amp = amps.get(&id).ok_or(AntNeuroError::NotConnected)?;
            modes.push(lock(amp)?.current_mode);
        }
        if !modes.windows(2).all(|w| w[0] == w[1]) {
            self.set_error("Not all devices are in the same state".to_string());
            return Err(AntNeuroError::IncorrectValue);
        }
        Ok(ids[0])
    }

    fn get_amplifier_serial(&self, id: i32) -> Result<String> {
        let amps = read_lock(&self.amplifiers)?;
        let amp = amps.get(&id).ok_or(AntNeuroError::NotConnected)?;
        let full = lock(amp)?.serial.clone();
        Ok(Self::parse_public_serial(&full))
    }

    fn get_amplifier_version(&self, id: i32) -> Result<i32> {
        let amps = read_lock(&self.amplifiers)?;
        let amp = amps.get(&id).ok_or(AntNeuroError::NotConnected)?;
        let v = lock(amp)?.firmware_version; Ok(v)
    }

    fn get_amplifier_type(&self, id: i32) -> Result<String> {
        let amps = read_lock(&self.amplifiers)?;
        let amp = amps.get(&id).ok_or(AntNeuroError::NotConnected)?;
        let v = lock(amp)?.device_type.clone(); Ok(v)
    }

    fn get_amplifier_power_state(&self, id: i32) -> Result<PowerState> {
        self.reject_if_eego24(id)?;
        let amps = read_lock(&self.amplifiers)?;
        let amp = amps.get(&id).ok_or(AntNeuroError::NotConnected)?;
        let state = lock(amp)?;
        Ok(PowerState {
            is_powered: state.is_powered,
            is_charging: state.is_charging,
            charging_level: state.charging_level,
        })
    }

    fn get_amplifier_channel_list(&self, id: i32) -> Result<Vec<Channel>> {
        self.reject_if_eego24(id)?;
        let amps = read_lock(&self.amplifiers)?;
        let amp = amps.get(&id).ok_or(AntNeuroError::NotConnected)?;
        let v = lock(amp)?.channels.clone(); Ok(v)
    }

    fn get_amplifier_sampling_rates_available(&self, id: i32) -> Result<Vec<i32>> {
        self.reject_if_eego24(id)?;
        let amps = read_lock(&self.amplifiers)?;
        let amp = amps.get(&id).ok_or(AntNeuroError::NotConnected)?;
        let v = lock(amp)?.sampling_rates.clone(); Ok(v)
    }

    fn get_amplifier_reference_ranges_available(&self, id: i32) -> Result<Vec<f64>> {
        self.reject_if_eego24(id)?;
        let amps = read_lock(&self.amplifiers)?;
        let amp = amps.get(&id).ok_or(AntNeuroError::NotConnected)?;
        let v = lock(amp)?.reference_ranges.clone(); Ok(v)
    }

    fn get_amplifier_bipolar_ranges_available(&self, id: i32) -> Result<Vec<f64>> {
        self.reject_if_eego24(id)?;
        let amps = read_lock(&self.amplifiers)?;
        let amp = amps.get(&id).ok_or(AntNeuroError::NotConnected)?;
        let v = lock(amp)?.bipolar_ranges.clone(); Ok(v)
    }

    fn open_eeg_stream(
        &self,
        amplifier_id: i32,
        sampling_rate: i32,
        reference_range: f64,
        bipolar_range: f64,
        channels: &[Channel],
    ) -> Result<i32> {
        self.reject_if_eego24(amplifier_id)?;
        let amps = read_lock(&self.amplifiers)?;
        let amp = amps.get(&amplifier_id).ok_or(AntNeuroError::NotConnected)?;
        let mut state = lock(amp)?;

        // Devices with no channels (EE301, auxusb) cannot host an EEG stream.
        if state.channels.is_empty() {
            return Err(AntNeuroError::AlreadyExists);
        }
        // EE213 / EE221 (32-channel reduced-config) only support impedance mode.
        if let Some(model) = Self::parse_model_num(&state.serial) {
            if model == 213 || model == 221 {
                return Err(AntNeuroError::InternalError);
            }
        }

        if state.current_mode != StreamingMode::Idle {
            self.set_error("Device is not idle".to_string());
            return Err(AntNeuroError::AlreadyExists);
        }

        if !state.sampling_rates.contains(&sampling_rate) {
            self.set_error(format!("unsupported sampling rate: {}", sampling_rate));
            return Err(AntNeuroError::IncorrectValue);
        }

        if !state.reference_ranges.iter().any(|&r| (r - reference_range).abs() < 1e-9) {
            self.set_error(format!("ref range {} not valid", reference_range));
            return Err(AntNeuroError::IncorrectValue);
        }
        // bipolar_range == 0.0 means "no bipolar channels requested".
        if bipolar_range != 0.0
            && !state.bipolar_ranges.iter().any(|&r| (r - bipolar_range).abs() < 1e-9)
        {
            self.set_error(format!("aux range {} not valid", bipolar_range));
            return Err(AntNeuroError::IncorrectValue);
        }

        // Configure hardware: set ranges, then rate, then enter streaming mode.
        log::info!(" setting signal range({}, {})", reference_range, bipolar_range);
        let ref_code = range_lut_code(&state.reference_ranges, reference_range);
        let bip_code = range_lut_code(&state.bipolar_ranges, bipolar_range);
        UsbCommand::set_range(&state.handle, false, ref_code)?;
        UsbCommand::set_range(&state.handle, true, bip_code)?;

        log::info!(" sample rate set to: {}", sampling_rate);
        UsbCommand::set_sampling_rate(&state.handle, sampling_rate as u32)?;

        log::info!(" streaming mode set to: Streaming");
        UsbCommand::set_streaming_mode(&state.handle, StreamingMode::Streaming)?;

        state.current_mode = StreamingMode::Streaming;

        let stream_id = self.next_stream_id.fetch_add(1, Ordering::SeqCst);
        let channel_count = channels.len();

        let buf_capacity = channel_count * sampling_rate as usize * RING_BUF_SECONDS;
        let ring_buffer = Arc::new(Mutex::new(RingBuffer::new(buf_capacity)));
        let parser: Arc<Mutex<Box<dyn DataParser>>> =
            Arc::new(Mutex::new(create_parser(state.family)));
        let frame_counter = Arc::new(AtomicU64::new(0));
        let loss_counter = Arc::new(AtomicU64::new(0));
        let active = Arc::new(AtomicBool::new(true));

        let thread_handle = spawn_streaming_thread(
            Arc::clone(&state.handle),
            state.ep_iso_in.unwrap_or(state.ep_bulk_in),
            channel_count,
            Arc::clone(&ring_buffer),
            Arc::clone(&parser),
            Arc::clone(&active),
            Arc::clone(&frame_counter),
            Arc::clone(&loss_counter),
            StreamingMode::Streaming,
            sampling_rate,
        );

        write_lock(&self.streams)?.insert(stream_id, StreamHandle {
            state: StreamState {
                amplifier_id,
                channels: channels.to_vec(),
                channel_count,
                sampling_rate,
                is_impedance: false,
                streaming_active: Arc::clone(&active),
                _streaming_thread: Some(thread_handle),
            },
            buffer: ring_buffer,
            parser,
            frame_counter,
            loss_counter,
        });

        Ok(stream_id)
    }

    fn open_impedance_stream(&self, amplifier_id: i32, channels: &[Channel]) -> Result<i32> {
        self.reject_if_eego24(amplifier_id)?;
        let amps = read_lock(&self.amplifiers)?;
        let amp = amps.get(&amplifier_id).ok_or(AntNeuroError::NotConnected)?;
        let mut state = lock(amp)?;

        // Devices with no channels cannot run impedance measurement.
        if state.channels.is_empty() {
            return Err(AntNeuroError::InternalError);
        }

        if state.current_mode != StreamingMode::Idle {
            self.set_error("Device is not idle".to_string());
            return Err(AntNeuroError::AlreadyExists);
        }

        log::info!(" streaming mode set to: Impedance");
        UsbCommand::set_streaming_mode(&state.handle, StreamingMode::Impedance)?;

        state.current_mode = StreamingMode::Impedance;

        let stream_id = self.next_stream_id.fetch_add(1, Ordering::SeqCst);
        let channel_count = channels.len();
        let buf_capacity = channel_count * 256;
        let ring_buffer = Arc::new(Mutex::new(RingBuffer::new(buf_capacity)));
        let parser: Arc<Mutex<Box<dyn DataParser>>> =
            Arc::new(Mutex::new(create_parser(state.family)));
        let frame_counter = Arc::new(AtomicU64::new(0));
        let loss_counter = Arc::new(AtomicU64::new(0));
        let active = Arc::new(AtomicBool::new(true));

        let thread_handle = spawn_streaming_thread(
            Arc::clone(&state.handle),
            state.ep_iso_in.unwrap_or(state.ep_bulk_in),
            channel_count,
            Arc::clone(&ring_buffer),
            Arc::clone(&parser),
            Arc::clone(&active),
            Arc::clone(&frame_counter),
            Arc::clone(&loss_counter),
            StreamingMode::Impedance,
            0,
        );

        write_lock(&self.streams)?.insert(stream_id, StreamHandle {
            state: StreamState {
                amplifier_id,
                channels: channels.to_vec(),
                channel_count,
                sampling_rate: 0,
                is_impedance: true,
                streaming_active: Arc::clone(&active),
                _streaming_thread: Some(thread_handle),
            },
            buffer: ring_buffer,
            parser,
            frame_counter,
            loss_counter,
        });

        Ok(stream_id)
    }

    fn close_stream(&self, stream_id: i32) -> Result<()> {
        if let Some(handle) = write_lock(&self.streams)?.remove(&stream_id) {
            let mut state = handle.state;
            state.streaming_active.store(false, Ordering::SeqCst);
            if let Some(thread) = state._streaming_thread.take() {
                let _ = thread.join();
            }

            let amps = read_lock(&self.amplifiers)?;
            if let Some(amp) = amps.get(&state.amplifier_id) {
                let mut amp_state = lock(amp)?;
                log::info!(" streaming mode set to: Idle");
                let _ = UsbCommand::set_streaming_mode(&amp_state.handle, StreamingMode::Idle);
                amp_state.current_mode = StreamingMode::Idle;
            }
        }

        Ok(())
    }

    fn get_stream_channel_list(&self, stream_id: i32) -> Result<Vec<Channel>> {
        let streams = read_lock(&self.streams)?;
        let handle = streams.get(&stream_id).ok_or(AntNeuroError::NotFound)?;
        let mut channels = handle.state.channels.clone();
        let next_idx = channels.last().map(|c| c.index + 1).unwrap_or(0);
        channels.push(Channel { index: next_idx, channel_type: ChannelType::Trigger });
        channels.push(Channel { index: next_idx + 1, channel_type: ChannelType::SampleCounter });
        Ok(channels)
    }

    fn get_stream_channel_count(&self, stream_id: i32) -> Result<usize> {
        let streams = read_lock(&self.streams)?;
        let handle = streams.get(&stream_id).ok_or(AntNeuroError::NotFound)?;
        Ok(handle.state.channel_count + 2)
    }

    fn prefetch(&self, stream_id: i32) -> Result<usize> {
        let streams = read_lock(&self.streams)?;
        let handle = streams.get(&stream_id).ok_or(AntNeuroError::NotFound)?;
        let available = lock(&handle.buffer)?.available();
        Ok(available * std::mem::size_of::<f64>())
    }

    fn get_data(&self, stream_id: i32, buffer: &mut [f64]) -> Result<usize> {
        let streams = read_lock(&self.streams)?;
        let handle = streams.get(&stream_id).ok_or(AntNeuroError::NotFound)?;
        let read = lock(&handle.buffer)?.read_into(buffer);
        if read == 0 {
            return Err(AntNeuroError::IncorrectValue);
        }
        Ok(read * std::mem::size_of::<f64>())
    }

    fn set_battery_charging(&self, amplifier_id: i32, flag: bool) -> Result<()> {
        let amps = read_lock(&self.amplifiers)?;
        let amp = amps.get(&amplifier_id).ok_or(AntNeuroError::NotConnected)?;
        let state = lock(amp)?;
        // Only the standard eego family (with channels) supports battery charging control.
        if state.family != DeviceFamily::Eego || state.channels.is_empty()
            || state.serial.starts_with("EE301")
        {
            return Err(AntNeuroError::InternalError);
        }
        UsbCommand::write(&state.handle, 0x31, if flag { 1 } else { 0 }, 0, &[])?;
        Ok(())
    }

    fn trigger_out_set_parameters(
        &self,
        amplifier_id: i32,
        _channel: i32,
        _duty_cycle: i32,
        _pulse_frequency: f32,
        _pulse_count: i32,
        _burst_frequency: f32,
        _burst_count: i32,
    ) -> Result<()> {
        // Only the eegomini family supports trigger output.
        self.trigger_out_family_check(amplifier_id)
    }

    fn trigger_out_start(&self, amplifier_id: i32, _channels: &[i32]) -> Result<()> {
        self.trigger_out_family_check(amplifier_id)
    }

    fn trigger_out_stop(&self, amplifier_id: i32, _channels: &[i32]) -> Result<()> {
        self.trigger_out_family_check(amplifier_id)
    }

    fn last_error(&self) -> Option<String> {
        read_lock(&self.last_error).ok().and_then(|e| e.clone())
    }
}

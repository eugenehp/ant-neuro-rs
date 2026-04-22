use std::sync::{
    atomic::{AtomicBool, AtomicU64},
    Arc, Mutex,
};
use std::thread;

use crate::channel::Channel;
use crate::protocol::{DeviceFamily, StreamingMode};
use crate::types::AmplifierInfo;

use super::ring_buffer::RingBuffer;
use super::ssp_parser::DataParser;

/// Per-device state: USB handle, capabilities, and current mode.
#[allow(dead_code)]
pub(crate) struct AmplifierState {
    pub(crate) handle: Arc<rusb::DeviceHandle<rusb::GlobalContext>>,
    pub(crate) info: AmplifierInfo,
    pub(crate) family: DeviceFamily,
    pub(crate) interface_num: u8,
    pub(crate) ep_bulk_in: u8,
    pub(crate) ep_bulk_out: u8,
    pub(crate) ep_iso_in: Option<u8>,
    pub(crate) iso_alt_setting: u8,
    pub(crate) usb_version: u16,
    pub(crate) channels: Vec<Channel>,
    pub(crate) sampling_rates: Vec<i32>,
    pub(crate) reference_ranges: Vec<f64>,
    pub(crate) bipolar_ranges: Vec<f64>,
    pub(crate) current_mode: StreamingMode,
    pub(crate) is_powered: bool,
    pub(crate) is_charging: bool,
    pub(crate) charging_level: i32,
    pub(crate) serial: String,
    pub(crate) device_type: String,
    pub(crate) firmware_version: i32,
}

/// All resources for one open stream, bundled together.
#[allow(dead_code)] // parser/frame_counter/loss_counter are Arc-cloned into the streaming thread
pub(crate) struct StreamHandle {
    pub(crate) state: StreamState,
    pub(crate) buffer: Arc<Mutex<RingBuffer>>,
    pub(crate) parser: Arc<Mutex<Box<dyn DataParser>>>,
    pub(crate) frame_counter: Arc<AtomicU64>,
    pub(crate) loss_counter: Arc<AtomicU64>,
}

/// Metadata and control handles for an active stream.
pub(crate) struct StreamState {
    pub(crate) amplifier_id: i32,
    pub(crate) channels: Vec<Channel>,
    pub(crate) channel_count: usize,
    #[allow(dead_code)] pub(crate) sampling_rate: i32,
    #[allow(dead_code)] pub(crate) is_impedance: bool,
    pub(crate) streaming_active: Arc<AtomicBool>,
    pub(crate) _streaming_thread: Option<thread::JoinHandle<()>>,
}

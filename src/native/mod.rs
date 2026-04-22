//! Native USB backend: pure-Rust implementation of the eego amplifier SDK.
//!
//! Architecture overview:
//!
//! ```text
//! NativeBackend       — device discovery and hotplug management
//! AmplifierState      — per-device state machine
//! StreamingThread     — real-time data acquisition loop
//! UsbCommand          — vendor USB control transfers
//! DataParser / SspParser — per-family SSP frame decoders
//! RingBuffer          — sample ring buffer for streaming data
//! ```
//!
//! USB transport: VID 0x2a56, PID 0xee01, Interface 0.
//! Control: vendor type (0x40 write, 0xC0 read), 3000ms timeout.
//! Streaming: isochronous transfers with SSP framing (sync, CRC).
//! Device families: eego, eego24, eegomini, auxusb (detected by serial regex).

mod usb_command;
mod ring_buffer;
mod ssp_parser;
mod streaming;
mod discovery;
mod state;
mod backend;
mod backend_impl;

pub use backend::NativeBackend;

use std::sync::{Mutex, RwLock};
use crate::error::{AntNeuroError, Result};

// ── Configuration constants ─────────────────────────────────────────────────

pub(crate) const USB_READ_TIMEOUT_MS: u64 = 100;
pub(crate) const USB_READ_BUF_SIZE: usize = 65536;
pub(crate) const RING_BUF_SECONDS: usize = 10;
#[allow(dead_code)]
pub(crate) const RECOVERY_SLEEP_MS: u64 = 100;

/// Acquire a read lock, mapping PoisonError to InternalError.
pub(crate) fn read_lock<T>(lock: &RwLock<T>) -> Result<std::sync::RwLockReadGuard<'_, T>> {
    lock.read().map_err(|_| AntNeuroError::InternalError)
}

/// Acquire a write lock, mapping PoisonError to InternalError.
pub(crate) fn write_lock<T>(lock: &RwLock<T>) -> Result<std::sync::RwLockWriteGuard<'_, T>> {
    lock.write().map_err(|_| AntNeuroError::InternalError)
}

/// Acquire a mutex lock, mapping PoisonError to InternalError.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>> {
    mutex.lock().map_err(|_| AntNeuroError::InternalError)
}

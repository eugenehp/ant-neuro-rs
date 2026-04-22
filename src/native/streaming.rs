// ── Streaming thread ─────────────────────────────────────────────────────────

use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

use crate::protocol::{self, StreamingMode};

use super::ring_buffer::RingBuffer;
use super::ssp_parser::DataParser;
use super::usb_command::UsbCommand;
use super::{USB_READ_BUF_SIZE, USB_READ_TIMEOUT_MS};

// ── Error recovery ──────────────────────────────────────────────────────────

/// Attempt to recover the streaming pipeline after a data stall.
///
/// Transitions the device to Idle, re-applies the sampling rate, and then
/// re-enters the target streaming mode.
pub(crate) fn attempt_recovery(
    handle: &rusb::DeviceHandle<rusb::GlobalContext>,
    mode: StreamingMode,
    rate: i32,
) {
    log::warn!("recovering stream mode {}", mode.name());

    log::debug!("recovery: idle");
    let _ = UsbCommand::set_streaming_mode(handle, StreamingMode::Idle);
    thread::sleep(Duration::from_millis(100));

    log::debug!("recovery: rate: {}", rate);
    let _ = UsbCommand::set_sampling_rate(handle, rate as u32);

    match mode {
        StreamingMode::Streaming => {
            log::debug!("recovery: mode: streaming");
            let _ = UsbCommand::set_streaming_mode(handle, StreamingMode::Streaming);
        }
        StreamingMode::Impedance => {
            log::debug!("recovery: mode: impedance");
            let _ = UsbCommand::set_streaming_mode(handle, StreamingMode::Impedance);
        }
        _ => {
            log::debug!("recovery: idle");
            let _ = UsbCommand::set_streaming_mode(handle, StreamingMode::Idle);
        }
    }

    log::debug!("recovery: done");
}

/// Maximum recovery attempts before giving up.
pub(crate) const MAX_RECOVERY_ATTEMPTS: u32 = 7;

/// Spawn a background thread that reads SSP frames from the USB endpoint,
/// decodes them, and pushes samples into the ring buffer.
pub(crate) fn spawn_streaming_thread(
    handle: Arc<rusb::DeviceHandle<rusb::GlobalContext>>,
    ep_iso_in: u8,
    channel_count: usize,
    ring_buffer: Arc<Mutex<RingBuffer>>,
    parser: Arc<Mutex<Box<dyn DataParser>>>,
    active: Arc<AtomicBool>,
    frame_counter: Arc<AtomicU64>,
    loss_counter: Arc<AtomicU64>,
    mode: StreamingMode,
    rate: i32,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("eego-stream".to_string())
        .spawn(move || {
            log::info!("streaming thread started");
            // We use read_bulk here even for the isochronous endpoint.
            // Under the LD_PRELOAD virtual-USB shim this routes through
            // vusb; on real hardware libusb handles the kernel-level
            // isochronous/bulk distinction transparently.
            let mut usb_buf = vec![0u8; USB_READ_BUF_SIZE];
            let mut parse_buf = Vec::with_capacity(4096);
            let mut last_data_time = Instant::now();
            let mut recovery_count: u32 = 0;

            while active.load(Ordering::SeqCst) {
                match handle.read_bulk(
                    ep_iso_in,
                    &mut usb_buf,
                    Duration::from_millis(USB_READ_TIMEOUT_MS),
                ) {
                    Ok(n) if n > 0 => {
                        last_data_time = Instant::now();
                        recovery_count = 0;
                        parse_buf.clear();

                        let Ok(mut p) = parser.lock() else { continue };
                        let num_samples = p.parse(&usb_buf[..n], channel_count, &mut parse_buf);
                        if num_samples > 0 {
                            let Ok(mut rb) = ring_buffer.lock() else { continue };
                            rb.push_slice(&parse_buf);
                            frame_counter.fetch_add(1, Ordering::Relaxed);
                        }
                        drop(p);
                    }
                    Ok(_) | Err(rusb::Error::Timeout) => {
                        if last_data_time.elapsed()
                            > Duration::from_secs(protocol::ISOC_WATCHDOG_TIMEOUT_SECS)
                        {
                            log::warn!(": USB isoc endpoint silent for 1 seconds");
                            if recovery_count < MAX_RECOVERY_ATTEMPTS {
                                // On repeated failures, fall back to 500 Hz.
                                let fallback_rate = if recovery_count > 0 { 500 } else { rate };
                                attempt_recovery(&handle, mode, fallback_rate);
                                recovery_count += 1;
                                loss_counter.fetch_add(1, Ordering::Relaxed);
                            } else {
                                log::error!(
                                    "clearing: no good data after {} tries",
                                    MAX_RECOVERY_ATTEMPTS
                                );
                                break;
                            }
                            last_data_time = Instant::now();
                        }
                    }
                    Err(e) => {
                        log::error!("usb_thread exception [{}]", e);
                        if !active.load(Ordering::SeqCst) {
                            break;
                        }
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            }

            log::info!("streaming thread stopped");
        })
        .expect("failed to spawn streaming thread")
}

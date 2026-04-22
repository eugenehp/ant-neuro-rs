// ── UsbCommand: vendor control transfer abstraction ──────────────────────────

use std::thread;
use std::time::Duration;

use crate::error::{AntNeuroError, Result};
use crate::protocol::{self, StreamingMode};

/// Low-level USB control transfer helpers for the eego amplifier protocol.
pub(crate) struct UsbCommand;

impl UsbCommand {
    /// Send a vendor control write (bmRequestType = 0x40) with retry.
    pub(crate) fn write(
        handle: &rusb::DeviceHandle<rusb::GlobalContext>,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
    ) -> Result<usize> {
        let timeout = Duration::from_millis(protocol::CONTROL_TIMEOUT_MS);
        let mut _last_err = None;

        for attempt in 0..protocol::COMMAND_MAX_RETRIES {
            match handle.write_control(
                protocol::VENDOR_WRITE,
                request,
                value,
                index,
                data,
                timeout,
            ) {
                Ok(n) => return Ok(n),
                Err(e) => {
                    log::warn!(
                        "error in sending eego command. attempt {} of {}: {}",
                        attempt + 1,
                        protocol::COMMAND_MAX_RETRIES,
                        e
                    );
                    _last_err = Some(e);
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }

        log::error!(
            "error in sending eego command. tried {} times",
            protocol::COMMAND_MAX_RETRIES
        );
        Err(AntNeuroError::InternalError)
    }

    /// Write-then-read control exchange.
    ///
    /// Sends an optional payload with the vendor-write request, then reads a
    /// 64-byte reply using `bReq_r = 0x02`. The reply contains a status word
    /// at byte offset 8; nonzero status indicates a device-side error.
    pub(crate) fn exchange(
        handle: &rusb::DeviceHandle<rusb::GlobalContext>,
        breq_w: u8,
        wval_w: u16,
        widx_w: u16,
        wbuf: &[u8],
    ) -> Result<[u8; 64]> {
        Self::write(handle, breq_w, wval_w, widx_w, wbuf)?;
        let timeout = Duration::from_millis(protocol::CONTROL_TIMEOUT_MS);
        let mut reply = [0u8; 64];
        match handle.read_control(protocol::VENDOR_READ, protocol::CMD_EXCHANGE_REPLY, 0, 0, &mut reply, timeout) {
            Ok(_) => {
                let status = u32::from_le_bytes([reply[8], reply[9], reply[10], reply[11]]);
                if status != 0 {
                    log::warn!("invalid control status: {}", status);
                }
                Ok(reply)
            }
            Err(e) => {
                log::warn!("could not read usb return status: {}", e);
                Err(AntNeuroError::InternalError)
            }
        }
    }

    /// Set the amplifier's sampling rate (Hz).
    pub(crate) fn set_sampling_rate(
        handle: &rusb::DeviceHandle<rusb::GlobalContext>,
        rate: u32,
    ) -> Result<()> {
        Self::exchange(handle, protocol::CMD_SET_SAMPLING_RATE, 0, 0, &rate.to_le_bytes())?;
        Ok(())
    }

    /// Set the reference or bipolar input range.
    ///
    /// `lut_code` is a hardware gain code selected from a fixed lookup table
    /// (see `range_lut_code`). The `bipolar` flag selects which range register
    /// to write: `wVal = 0x04` for reference, `wVal = 0x08` for bipolar.
    pub(crate) fn set_range(
        handle: &rusb::DeviceHandle<rusb::GlobalContext>,
        bipolar: bool,
        lut_code: u32,
    ) -> Result<()> {
        let wval = if bipolar { 0x0008 } else { 0x0004 };
        Self::exchange(handle, protocol::CMD_SET_RANGE, wval, 0, &lut_code.to_le_bytes())?;
        Ok(())
    }

    /// Set the device streaming mode (Idle, Streaming, Calibration, or Impedance).
    ///
    /// Each mode maps to a wire code: Idle=1, Streaming=2, Calibration=3, Impedance=5.
    pub(crate) fn set_streaming_mode(
        handle: &rusb::DeviceHandle<rusb::GlobalContext>,
        mode: StreamingMode,
    ) -> Result<()> {
        let wire: u32 = match mode {
            StreamingMode::Idle => 1,
            StreamingMode::Streaming => 2,
            StreamingMode::Calibration => 3,
            StreamingMode::Impedance => 5,
            StreamingMode::None => 0,
        };
        Self::exchange(handle, protocol::CMD_SET_STREAMING_MODE, 0, 0, &wire.to_le_bytes())?;
        Ok(())
    }
}

/// Map a public range value (in volts) to its hardware gain LUT code.
///
/// The LUT is a fixed 6-entry table `[2, 3, 4, 6, 8, 12]` indexed by the
/// position of the target range in the device's supported range list.
/// Returns 0 if the range is not found (callers should validate beforehand).
pub(crate) fn range_lut_code(ranges: &[f64], target: f64) -> u32 {
    const LUT: [u32; 6] = [2, 3, 4, 6, 8, 12];
    ranges
        .iter()
        .position(|&r| (r - target).abs() < 1e-9)
        .and_then(|i| LUT.get(i).copied())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_range_lut_code_first() {
        let ranges = &[4.0, 1.5, 0.7, 0.35];
        assert_eq!(range_lut_code(ranges, 4.0), 2);
    }

    #[test]
    fn test_range_lut_code_second() {
        let ranges = &[4.0, 1.5, 0.7, 0.35];
        assert_eq!(range_lut_code(ranges, 1.5), 3);
    }

    #[test]
    fn test_range_lut_code_third() {
        let ranges = &[4.0, 1.5, 0.7, 0.35];
        assert_eq!(range_lut_code(ranges, 0.7), 4);
    }

    #[test]
    fn test_range_lut_code_fourth() {
        let ranges = &[4.0, 1.5, 0.7, 0.35];
        assert_eq!(range_lut_code(ranges, 0.35), 6);
    }

    #[test]
    fn test_range_lut_code_not_found() {
        let ranges = &[4.0, 1.5, 0.7, 0.35];
        assert_eq!(range_lut_code(ranges, 999.0), 0);
    }

    #[test]
    fn test_range_lut_code_empty_ranges() {
        let ranges: &[f64] = &[];
        assert_eq!(range_lut_code(ranges, 1.0), 0);
    }

    #[test]
    fn test_range_lut_code_beyond_lut_size() {
        // LUT only has 6 entries; if ranges has > 6 elements and we match index >= 6, return 0
        let ranges = &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        assert_eq!(range_lut_code(ranges, 7.0), 0); // index 6, beyond LUT
    }
}

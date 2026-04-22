//! eego USB protocol constants and types.
//!
//! # How the eego hardware communicates
//!
//! The eego amplifier connects via USB using a Cypress FX3 controller. The
//! host talks to it through two mechanisms:
//!
//! 1. **Control transfers** (the "command channel") — short request/response
//!    pairs for configuration: set the sampling rate, start streaming, query
//!    battery status, etc. Each command is a USB "vendor request" identified
//!    by a one-byte command number.
//!
//! 2. **Isochronous transfers** (the "data channel") — a continuous stream
//!    of sample data wrapped in SSP frames (see below).
//!
//! Additionally, the device has a **bulk endpoint** that carries 72-byte
//! status/event messages parsed by `eego_message_parser`.
//!
//! # Command numbers (USB bRequest field)
//!
//! Every command follows a "direct exchange" pattern:
//! 1. Host sends a vendor-write with the command number and optional payload.
//! 2. Host reads back a 64-byte reply (command number 0x02) containing a
//!    status word at byte offset 8 (0 = success).
//!
//! | Command | Hex  | Payload | Purpose |
//! |---------|------|---------|---------|
//! | [`CMD_DEVICE_PROBE`] | 0x00 | — | Device init probe (sent twice: param 1 and 2) |
//! | [`CMD_CHANNEL_LIST`] | 0x01 | — | Request channel list |
//! | [`CMD_EXCHANGE_REPLY`] | 0x02 | — | Read the 64-byte reply buffer |
//! | [`CMD_SET_STREAMING_MODE`] | 0x10 | u32 wire code | Set streaming mode (1=Idle, 2=EEG, 3=Cal, 5=Impedance) |
//! | [`CMD_SET_SAMPLING_RATE`] | 0x14 | u32 Hz | Set sampling rate |
//! | [`CMD_SET_RANGE`] | 0x15 | u32 gain code | Set reference (param=4) or bipolar (param=8) range |
//! | [`CMD_BATTERY_STATUS`] | 0x50 | — | Query battery (reply byte 4 must equal 4) |
//! | [`CMD_GET_STREAMING_MODE`] | 0x90 | — | Read current streaming mode (reply byte 20) |
//!
//! # SSP — Serial Streaming Protocol
//!
//! SSP is ANT Neuro's internal framing protocol for the isochronous data
//! channel. The name is inferred from string tags in the binary (`[ssp sync]`,
//! `[ssp crc]`, `[ssp receive]`, `[ssp transmit]`) — it is not a public
//! standard.
//!
//! Each device family has a different frame format:
//!
//! | Family | Frame size | Sync pattern | Samples per frame |
//! |--------|-----------|--------------|-------------------|
//! | eego (EE2xx) | 408 bytes | `A5 A5 A5 A5` at end | 24 × 32-bit signed integers |
//! | eegomini (EE5xx) | variable (ch×28+10) | `A5 A5 A5 A5` at end | ch × 7 × 16-bit signed |
//! | eego24 (EE4xx) | 68 bytes | `C5 C5 C5 C5` at start and end | 8 × 32-bit floats |
//!
//! # Glossary
//!
//! | Term | Meaning |
//! |------|---------|
//! | SSP | Serial Streaming Protocol — the frame format on the isochronous endpoint |
//! | VID / PID | USB Vendor ID (`0x2a56`) and Product ID (`0xee01`) |
//! | FX3 | Cypress EZ-USB FX3 — the USB 3.0 controller chip on the eego PCB |
//! | LPC | NXP LPC — the microcontroller running the ADC firmware |
//! | LUT | Look-Up Table — gain codes `{2, 3, 4, 6, 8, 12}` sent with [`CMD_SET_RANGE`] |
//! | Wire code | The small integer libeego sends over USB for a streaming mode (1–5), as opposed to the enum value (10–13) used in the API |

// ── USB identifiers ─────────────────────────────────────────────────────────

/// USB Vendor ID for ANT Neuro / eemagine devices.
pub const EEGO_VID: u16 = 0x2a56;

/// USB Product ID for the eego amplifier.
pub const EEGO_PID: u16 = 0xee01;

/// The USB interface number used by all eego devices (always 0).
pub const EEGO_INTERFACE: u8 = 0;

// ── USB transfer parameters ─────────────────────────────────────────────────

/// Timeout for USB control transfers, in milliseconds.
pub const CONTROL_TIMEOUT_MS: u64 = 3000;

/// USB request-type byte for a vendor write (host → device).
pub const VENDOR_WRITE: u8 = 0x40;

/// USB request-type byte for a vendor read (device → host).
pub const VENDOR_READ: u8 = 0xC0;

/// Isochronous bandwidth multiplier for USB 2.0 (High Speed).
pub const USB2_BANDWIDTH_MULTIPLIER: u32 = 1000;

/// Isochronous bandwidth multiplier for USB 3.0 (SuperSpeed).
pub const USB3_BANDWIDTH_MULTIPLIER: u32 = 8000;

/// USB version word for USB 2.0 comparison.
pub const USB_VERSION_2_0: u16 = 0x0200;

// ── Command numbers ─────────────────────────────────────────────────────────
//
// These are the bRequest values in the USB control transfer setup packet.
// "bRequest" is the USB spec's name for "which command to run."

/// Device constructor probe — sent at init with parameter 1, then parameter 2.
pub const CMD_DEVICE_PROBE: u8 = 0x00;

/// Channel list exchange (write-leg).
pub const CMD_CHANNEL_LIST: u8 = 0x01;

/// Read the 64-byte exchange reply buffer. Every command above is followed
/// by a read of this command to retrieve the device's response.
pub const CMD_EXCHANGE_REPLY: u8 = 0x02;

/// Set the streaming mode. Payload is a u32 "wire code":
/// 1 = Idle, 2 = Streaming (EEG), 3 = Calibration, 5 = Impedance.
pub const CMD_SET_STREAMING_MODE: u8 = 0x10;

/// Set the sampling rate. Payload is a u32 rate in Hz.
pub const CMD_SET_SAMPLING_RATE: u8 = 0x14;

/// Set the voltage range. Parameter field selects reference (4) or bipolar (8).
/// Payload is a u32 gain code from the LUT: `{2, 3, 4, 6, 8, 12}`.
pub const CMD_SET_RANGE: u8 = 0x15;

/// Query battery status. Reply byte 4 must equal 4 for success.
pub const CMD_BATTERY_STATUS: u8 = 0x50;

/// Read the current streaming mode. Reply byte 20 contains the wire code.
pub const CMD_GET_STREAMING_MODE: u8 = 0x90;

// ── Retry / watchdog constants ──────────────────────────────────────────────

/// How many times to retry a failed USB command before giving up.
pub const COMMAND_MAX_RETRIES: u32 = 3;

/// Seconds of silence on the isochronous endpoint before triggering recovery.
pub const ISOC_WATCHDOG_TIMEOUT_SECS: u64 = 1;

// ── SDK version ─────────────────────────────────────────────────────────────

/// eego SDK version as a build number. Version 1.3.29 → build 57168.
pub const SDK_VERSION: i32 = 57168;
pub const SDK_VERSION_MAJOR: i32 = 1;
pub const SDK_VERSION_MINOR: i32 = 3;
pub const SDK_VERSION_MICRO: i32 = 29;

/// Cypress USB device GUID (used on Windows for driver matching).
pub const CYPRESS_GUID: &str = "01D0609B-729E-459D-9E96-9805D19E6B8B";

// ── Device families ─────────────────────────────────────────────────────────

/// Device family, determined by matching the serial number against known
/// regex patterns from the vendor SDK.
///
/// | Pattern | Family |
/// |---------|--------|
/// | `EE2xx-*` | [`Eego`](DeviceFamily::Eego) |
/// | `EE301-*` | [`Eego`](DeviceFamily::Eego) |
/// | `EE4xx-*` | [`Eego24`](DeviceFamily::Eego24) |
/// | `EE5xx-*` or `Mxxxx-*` | [`EegoMini`](DeviceFamily::EegoMini) |
/// | `ZZ0xx-*` | [`AuxUsb`](DeviceFamily::AuxUsb) |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceFamily {
    /// Original eego (EE2xx, EE301). 64 reference + 24 bipolar channels.
    Eego,
    /// eego24 (EE4xx). 24-channel variant.
    Eego24,
    /// eegomini (EE5xx, Mxxxx). Compact 8-electrode variant.
    EegoMini,
    /// Auxiliary USB device (ZZ0xx). External trigger/aux box.
    AuxUsb,
    /// Unrecognized serial number pattern.
    Unknown,
}

impl DeviceFamily {
    /// Detect the device family from a serial number string.
    pub fn from_serial(serial: &str) -> Self {
        if serial.starts_with("EE2") && serial.len() >= 5 && serial[2..5].chars().all(|c| c.is_ascii_digit()) {
            Self::Eego
        } else if serial.starts_with("EE301") {
            Self::Eego
        } else if serial.starts_with("EE4") && serial.len() >= 5 && serial[2..5].chars().all(|c| c.is_ascii_digit()) {
            Self::Eego24
        } else if serial.starts_with("EE5") && serial.len() >= 5 && serial[2..5].chars().all(|c| c.is_ascii_digit()) {
            Self::EegoMini
        } else if serial.starts_with('M') && serial.len() >= 5 && serial[1..5].chars().all(|c| c.is_ascii_digit()) {
            Self::EegoMini
        } else if serial.starts_with("ZZ0") && serial.len() >= 5 && serial[2..5].chars().all(|c| c.is_ascii_digit()) {
            Self::AuxUsb
        } else if serial.len() >= 5 && serial[..5].chars().all(|c| c.is_ascii_digit()) {
            Self::Eego // legacy numeric serials
        } else {
            Self::Unknown
        }
    }

    /// Human-readable family name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Eego => "eego",
            Self::Eego24 => "eego24",
            Self::EegoMini => "eegomini",
            Self::AuxUsb => "auxusb",
            Self::Unknown => "unknown",
        }
    }
}

// ── Streaming modes ─────────────────────────────────────────────────────────

/// Streaming mode, as represented in the high-level API.
///
/// The enum values (10, 11, 12, 13) are the **API-level** codes used inside
/// libeego. On the USB wire, these are translated to smaller "wire codes"
/// (1, 2, 3, 5) by [`CMD_SET_STREAMING_MODE`]. The translation happens in
/// [`UsbCommand::set_streaming_mode`](crate::native).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StreamingMode {
    None = 0,
    Idle = 10,
    Streaming = 11,
    Calibration = 12,
    Impedance = 13,
}

impl StreamingMode {
    pub fn name(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Idle => "Idle",
            Self::Streaming => "Streaming",
            Self::Calibration => "Calibration",
            Self::Impedance => "Impedance",
        }
    }
}

// ── Power states ────────────────────────────────────────────────────────────

/// Power state reported by the device. Used internally; the public API
/// exposes [`PowerState`](crate::types::PowerState) instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerStatus {
    Powered,
    Unpowered,
}

// ── Signal ranges ───────────────────────────────────────────────────────────

/// Default reference voltage ranges (Volts) for the eego family.
pub const REFERENCE_RANGES: &[f64] = &[1.0, 0.75, 0.15];

/// Default bipolar voltage ranges (Volts) for the eego family.
/// The eegomini uses a different set: `[2.5, 1.875, 0.375]`.
pub const BIPOLAR_RANGES: &[f64] = &[4.0, 1.5, 0.7, 0.35];

/// All sampling rates the hardware supports (Hz). Per-device max rates
/// filter this list down to what's actually available.
pub const ALL_SAMPLING_RATES: &[i32] = &[
    500, 512, 1000, 1024, 2000, 2048, 4000, 4096, 8000, 8192, 16000, 16384,
];

// ── SSP tags ────────────────────────────────────────────────────────────────
//
// These strings appear in libeego's command-channel error reporting, not
// in the actual SSP frame format. See `native/ssp_parser.rs` for frame layouts.

/// SSP error tag strings from the libeego binary.
pub mod ssp {
    pub const TAG_SYNC: &str = "ssp sync";
    pub const TAG_CRC: &str = "ssp crc";
    pub const TAG_RECEIVE: &str = "ssp receive";
    pub const TAG_TRANSMIT: &str = "ssp transmit";
}

// ── Environment variables ───────────────────────────────────────────────────

/// Set this env var to disable eego device discovery.
pub const ENV_EEGO_DISABLE: &str = "EEGO_SDK_EEGO_DISABLE";
pub const ENV_LSL_IN: &str = "EEGO_SDK_LSL_IN";
pub const ENV_LSL_OUT: &str = "EEGO_SDK_LSL_OUT";
/// Set to a file path to enable libeego's internal error log.
pub const ENV_ERROR_HISTORY_LOG: &str = "EDI_ERROR_HISTORY_LOG";

// ── Firmware components ─────────────────────────────────────────────────────

/// The two processors on the eego board.
#[derive(Debug, Clone, Copy)]
pub enum FirmwareComponent {
    /// Cypress FX3 USB controller.
    Fx3,
    /// NXP LPC microcontroller (runs the ADC).
    Lpc,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_family_from_serial_ee225() {
        assert_eq!(DeviceFamily::from_serial("EE225-00042-00000001"), DeviceFamily::Eego);
    }

    #[test]
    fn test_family_from_serial_ee301() {
        assert_eq!(DeviceFamily::from_serial("EE301-00001-00000001"), DeviceFamily::Eego);
    }

    #[test]
    fn test_family_from_serial_ee410() {
        assert_eq!(DeviceFamily::from_serial("EE410-00001-00000001"), DeviceFamily::Eego24);
    }

    #[test]
    fn test_family_from_serial_ee520() {
        assert_eq!(DeviceFamily::from_serial("EE520-00001-00000001"), DeviceFamily::EegoMini);
    }

    #[test]
    fn test_family_from_serial_m0001() {
        assert_eq!(DeviceFamily::from_serial("M0001-00001-00000001"), DeviceFamily::EegoMini);
    }

    #[test]
    fn test_family_from_serial_zz001() {
        assert_eq!(DeviceFamily::from_serial("ZZ001-00001-00000001"), DeviceFamily::AuxUsb);
    }

    #[test]
    fn test_family_from_serial_numeric() {
        assert_eq!(DeviceFamily::from_serial("12345-00001"), DeviceFamily::Eego);
    }

    #[test]
    fn test_family_from_serial_unknown() {
        assert_eq!(DeviceFamily::from_serial("XY123"), DeviceFamily::Unknown);
    }

    #[test]
    fn test_family_from_serial_short_string() {
        assert_eq!(DeviceFamily::from_serial("EE2"), DeviceFamily::Unknown);
    }

    #[test]
    fn test_streaming_mode_names() {
        assert_eq!(StreamingMode::None.name(), "None");
        assert_eq!(StreamingMode::Idle.name(), "Idle");
        assert_eq!(StreamingMode::Streaming.name(), "Streaming");
        assert_eq!(StreamingMode::Calibration.name(), "Calibration");
        assert_eq!(StreamingMode::Impedance.name(), "Impedance");
    }

    #[test]
    fn test_cmd_constants() {
        assert_eq!(CMD_DEVICE_PROBE, 0x00);
        assert_eq!(CMD_CHANNEL_LIST, 0x01);
        assert_eq!(CMD_EXCHANGE_REPLY, 0x02);
        assert_eq!(CMD_SET_STREAMING_MODE, 0x10);
        assert_eq!(CMD_SET_SAMPLING_RATE, 0x14);
        assert_eq!(CMD_SET_RANGE, 0x15);
        assert_eq!(CMD_BATTERY_STATUS, 0x50);
        assert_eq!(CMD_GET_STREAMING_MODE, 0x90);
    }

    #[test]
    fn test_family_name() {
        assert_eq!(DeviceFamily::Eego.name(), "eego");
        assert_eq!(DeviceFamily::Eego24.name(), "eego24");
        assert_eq!(DeviceFamily::EegoMini.name(), "eegomini");
        assert_eq!(DeviceFamily::AuxUsb.name(), "auxusb");
        assert_eq!(DeviceFamily::Unknown.name(), "unknown");
    }

    #[test]
    fn test_usb_identifiers() {
        assert_eq!(EEGO_VID, 0x2a56);
        assert_eq!(EEGO_PID, 0xee01);
    }
}

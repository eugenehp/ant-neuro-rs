//! Raw FFI bindings to the eego SDK shared library.
//! Loaded dynamically at runtime via `libloading`.

use libloading::{Library, Symbol};
use std::path::Path;

use crate::channel::RawChannelInfo;
use crate::error::{AntNeuroError, Result};
use crate::types::RawAmplifierInfo;

/// Expected SDK build version for compatibility check.
const EEGO_SDK_VERSION: i32 = 57168;

/// Holds the dynamically loaded library and all resolved function pointers.
#[allow(dead_code)]
pub(crate) struct SdkBindings {
    // Keep the library alive as long as bindings exist.
    _lib: Library,

    // Library lifecycle
    pub init: unsafe extern "C" fn(),
    pub exit: unsafe extern "C" fn(),
    pub get_version: unsafe extern "C" fn() -> i32,

    // Amplifier discovery
    pub get_amplifiers_info:
        unsafe extern "C" fn(*mut RawAmplifierInfo, i32) -> i32,
    pub open_amplifier: unsafe extern "C" fn(i32) -> i32,
    pub close_amplifier: unsafe extern "C" fn(i32) -> i32,
    pub create_cascaded_amplifier: unsafe extern "C" fn(*const i32, i32) -> i32,

    // Amplifier info
    pub get_amplifier_serial: unsafe extern "C" fn(i32, *mut u8, i32) -> i32,
    pub get_amplifier_version: unsafe extern "C" fn(i32) -> i32,
    pub get_amplifier_type: unsafe extern "C" fn(i32, *mut u8, i32) -> i32,
    pub get_amplifier_power_state:
        unsafe extern "C" fn(i32, *mut i32, *mut i32, *mut i32) -> i32,
    pub get_amplifier_channel_list:
        unsafe extern "C" fn(i32, *mut RawChannelInfo, i32) -> i32,
    pub get_amplifier_sampling_rates_available:
        unsafe extern "C" fn(i32, *mut i32, i32) -> i32,
    pub get_amplifier_reference_ranges_available:
        unsafe extern "C" fn(i32, *mut f64, i32) -> i32,
    pub get_amplifier_bipolar_ranges_available:
        unsafe extern "C" fn(i32, *mut f64, i32) -> i32,

    // Streams
    pub open_eeg_stream:
        unsafe extern "C" fn(i32, i32, f64, f64, *const RawChannelInfo, i32) -> i32,
    pub open_impedance_stream:
        unsafe extern "C" fn(i32, *const RawChannelInfo, i32) -> i32,
    pub close_stream: unsafe extern "C" fn(i32) -> i32,
    pub get_stream_channel_list:
        unsafe extern "C" fn(i32, *mut RawChannelInfo, i32) -> i32,
    pub get_stream_channel_count: unsafe extern "C" fn(i32) -> i32,
    pub prefetch: unsafe extern "C" fn(i32) -> i32,
    pub get_data: unsafe extern "C" fn(i32, *mut f64, i32) -> i32,

    // Battery & trigger
    pub set_battery_charging: unsafe extern "C" fn(i32, i32) -> i32,
    pub trigger_out_set_parameters:
        unsafe extern "C" fn(i32, i32, i32, f32, i32, f32, i32) -> i32,
    pub trigger_out_start: unsafe extern "C" fn(i32, *const i32, i32) -> i32,
    pub trigger_out_stop: unsafe extern "C" fn(i32, *const i32, i32) -> i32,

    // Error
    pub get_error_string: unsafe extern "C" fn(*mut u8, i32) -> i32,
}

macro_rules! load_fn {
    ($lib:expr, $name:expr) => {{
        let sym: Symbol<*const ()> = $lib.get($name.as_bytes())?;
        std::mem::transmute(*sym)
    }};
}

impl SdkBindings {
    /// Load the eego SDK from the given shared library path.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        unsafe {
            let lib = Library::new(path.as_ref())?;

            let get_version: unsafe extern "C" fn() -> i32 = {
                let sym: Symbol<*const ()> =
                    lib.get(b"eemagine_sdk_get_version")?;
                std::mem::transmute(*sym)
            };

            let version = get_version();
            if version != EEGO_SDK_VERSION {
                return Err(AntNeuroError::VersionMismatch {
                    expected: EEGO_SDK_VERSION,
                    actual: version,
                });
            }

            let bindings = SdkBindings {
                init: load_fn!(lib, "eemagine_sdk_init"),
                exit: load_fn!(lib, "eemagine_sdk_exit"),
                get_version,
                get_amplifiers_info: load_fn!(lib, "eemagine_sdk_get_amplifiers_info"),
                open_amplifier: load_fn!(lib, "eemagine_sdk_open_amplifier"),
                close_amplifier: load_fn!(lib, "eemagine_sdk_close_amplifier"),
                create_cascaded_amplifier: load_fn!(lib, "eemagine_sdk_create_cascaded_amplifier"),
                get_amplifier_serial: load_fn!(lib, "eemagine_sdk_get_amplifier_serial"),
                get_amplifier_version: load_fn!(lib, "eemagine_sdk_get_amplifier_version"),
                get_amplifier_type: load_fn!(lib, "eemagine_sdk_get_amplifier_type"),
                get_amplifier_power_state: load_fn!(lib, "eemagine_sdk_get_amplifier_power_state"),
                get_amplifier_channel_list: load_fn!(
                    lib,
                    "eemagine_sdk_get_amplifier_channel_list"
                ),
                get_amplifier_sampling_rates_available: load_fn!(
                    lib,
                    "eemagine_sdk_get_amplifier_sampling_rates_available"
                ),
                get_amplifier_reference_ranges_available: load_fn!(
                    lib,
                    "eemagine_sdk_get_amplifier_reference_ranges_available"
                ),
                get_amplifier_bipolar_ranges_available: load_fn!(
                    lib,
                    "eemagine_sdk_get_amplifier_bipolar_ranges_available"
                ),
                open_eeg_stream: load_fn!(lib, "eemagine_sdk_open_eeg_stream"),
                open_impedance_stream: load_fn!(lib, "eemagine_sdk_open_impedance_stream"),
                close_stream: load_fn!(lib, "eemagine_sdk_close_stream"),
                get_stream_channel_list: load_fn!(lib, "eemagine_sdk_get_stream_channel_list"),
                get_stream_channel_count: load_fn!(lib, "eemagine_sdk_get_stream_channel_count"),
                prefetch: load_fn!(lib, "eemagine_sdk_prefetch"),
                get_data: load_fn!(lib, "eemagine_sdk_get_data"),
                set_battery_charging: load_fn!(lib, "eemagine_sdk_set_battery_charging"),
                trigger_out_set_parameters: load_fn!(
                    lib,
                    "eemagine_sdk_trigger_out_set_parameters"
                ),
                trigger_out_start: load_fn!(lib, "eemagine_sdk_trigger_out_start"),
                trigger_out_stop: load_fn!(lib, "eemagine_sdk_trigger_out_stop"),
                get_error_string: load_fn!(lib, "eemagine_sdk_get_error_string"),
                _lib: lib,
            };

            (bindings.init)();

            Ok(bindings)
        }
    }

    /// Get the last error string from the SDK.
    pub fn last_error(&self) -> Option<String> {
        let mut buf = [0u8; 512];
        let rc = unsafe { (self.get_error_string)(buf.as_mut_ptr(), buf.len() as i32) };
        if rc > 0 {
            Some(
                String::from_utf8_lossy(&buf[..rc as usize])
                    .trim_end_matches('\0')
                    .to_string(),
            )
        } else {
            None
        }
    }
}

impl Drop for SdkBindings {
    fn drop(&mut self) {
        unsafe {
            (self.exit)();
        }
    }
}

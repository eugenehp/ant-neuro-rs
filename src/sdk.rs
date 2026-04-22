use std::path::Path;
use std::sync::Arc;

use crate::amplifier::Amplifier;
use crate::backend::Backend;
use crate::error::{AntNeuroError, Result};
use crate::simulator::{SimulatorBackend, SimulatorConfig};
use crate::types::AmplifierInfo;

/// Entry point for the ANT Neuro SDK.
///
/// Automatically selects the best available backend:
/// 1. FFI backend (if `ffi` feature enabled and vendor library found)
/// 2. Native USB backend (if `native` feature enabled)
/// 3. Error if neither is available
pub struct AntNeuroSdk {
    backend: Arc<dyn Backend>,
}

impl AntNeuroSdk {
    /// Create a new SDK instance. Tries the vendor library first (if `ffi`
    /// feature is enabled), then falls back to the native USB backend.
    pub fn new(library_path: impl AsRef<Path>) -> Result<Self> {
        #[cfg(feature = "ffi")]
        {
            match crate::ffi_backend::FfiBackend::load(library_path.as_ref()) {
                Ok(ffi) => {
                    log::info!("Loaded vendor SDK library");
                    return Ok(Self { backend: Arc::new(ffi) });
                }
                Err(e) => {
                    log::info!("Vendor SDK not available ({}), trying native backend", e);
                }
            }
        }
        let _ = library_path; // suppress unused warning when ffi is off
        Self::new_native()
    }

    /// Create a new SDK instance using the native USB backend directly.
    /// No vendor shared library required.
    pub fn new_native() -> Result<Self> {
        #[cfg(feature = "native")]
        {
            let native = crate::native::NativeBackend::new()?;
            return Ok(Self { backend: Arc::new(native) });
        }
        #[cfg(not(feature = "native"))]
        {
            Err(AntNeuroError::InternalError)
        }
    }

    /// Get a reference to the underlying backend.
    pub fn backend(&self) -> &dyn Backend {
        self.backend.as_ref()
    }

    /// Get a shared reference to the backend (for Amplifier/Stream).
    pub(crate) fn backend_arc(&self) -> Arc<dyn Backend> {
        Arc::clone(&self.backend)
    }

    /// Discover all connected amplifiers without opening them.
    pub fn get_amplifiers_info(&self) -> Result<Vec<AmplifierInfo>> {
        self.backend.get_amplifiers_info()
    }

    /// Open a specific amplifier by ID.
    pub fn open_amplifier(&self, id: i32) -> Result<Amplifier> {
        self.backend.open_amplifier(id)?;
        Ok(Amplifier {
            backend: self.backend_arc(),
            id,
        })
    }

    /// Open the first available amplifier.
    pub fn open_first_amplifier(&self) -> Result<Amplifier> {
        let infos = self.get_amplifiers_info()?;
        let first = infos.first().ok_or(AntNeuroError::NoAmplifiers)?;
        self.open_amplifier(first.id)
    }

    /// Create a virtual cascaded amplifier from multiple amplifiers.
    pub fn create_cascaded_amplifier(&self, amplifiers: Vec<Amplifier>) -> Result<Amplifier> {
        let ids: Vec<i32> = amplifiers.iter().map(|a| a.id).collect();
        for mut _a in amplifiers {
            _a.id = -1; // prevent Drop from closing
        }
        let new_id = self.backend.create_cascaded_amplifier(&ids)?;
        Ok(Amplifier {
            backend: self.backend_arc(),
            id: new_id,
        })
    }

    /// Get the last error message from the SDK.
    pub fn last_error(&self) -> Option<String> {
        self.backend.last_error()
    }

    /// Create a new SDK instance using the simulated backend.
    /// No USB hardware needed. Generates synthetic EEG/impedance data.
    pub fn new_simulated(configs: Vec<SimulatorConfig>) -> Result<Self> {
        let sim = SimulatorBackend::new(configs)?;
        Ok(Self {
            backend: Arc::new(sim),
        })
    }

    /// Create with a single default simulated amplifier.
    pub fn new_simulated_default() -> Result<Self> {
        let sim = SimulatorBackend::new_default()?;
        Ok(Self {
            backend: Arc::new(sim),
        })
    }

    /// Get the SDK version number.
    pub fn version(&self) -> i32 {
        self.backend.get_version()
    }
}

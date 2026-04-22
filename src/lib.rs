//! # antneuro
//!
//! Rust SDK and terminal UI for streaming EEG data from
//! [ANT Neuro](https://www.ant-neuro.com/) eego amplifiers via the eego SDK
//! shared library.
//!
//! ## Supported amplifiers
//!
//! | Model | REF channels | BIP | Trigger | Max rate | Connection |
//! |---|---|---|---|---|---|
//! | eego mylab | 32–256 | 24 | 8-bit TTL | 16,384 Hz | USB-C 3.0 |
//! | eego sport 24 | 8–24 | — | 2-bit TTL | 2,048 Hz | USB-C 2.0 |
//! | eego sport 32/64 | 32–64 | — | 8-bit TTL | 16,384 Hz | USB-C 3.0 |
//! | eego rt 8 | 8 | 24 | 2-bit TTL | 2,048 Hz | USB 2.0 |
//! | eego rt 32/64 | 32–64 | 24 | 8-bit TTL | 16,384 Hz | USB 2.0 |
//!
//! All: 24-bit resolution, >1 GOhm input impedance, <1.0 uVRMS noise, CMRR >100 dB.
//!
//! ## Supported EEG caps (waveguard)
//!
//! | Cap | Channels | Electrode type | Application |
//! |---|---|---|---|
//! | waveguard original | 24–256 | Ag/AgCl gel | Research, clinical |
//! | waveguard touch | 32–64 | Ag/AgCl dry multi-pin | Rapid setup, no gel |
//! | waveguard connect | 21, 25 | Tin passive (silicone) | Clinical routine |
//! | waveguard net | 24–256 | Saline sponge (gel-free) | High-density, rapid |
//!
//! ## Electrode positions
//!
//! **21-channel (standard 10-20):** Fp1, Fp2, F3, F4, C3, C4, P3, P4, O1, O2,
//! F7, F8, T7, T8, P7, P8, Fz, Cz, Pz + ref (CPz) + gnd (AFz)
//!
//! **25-channel (IFCN):** Standard 10-20 + F9, F10, T9, T10, P9, P10
//!
//! **32-channel:** Extended 10-20 with FC, CP, PO, and Oz positions
//!
//! **64-channel:** Full 10-10 system (AF, F, FC, C, CP, P, PO rows, odd=left, even=right, z=midline)
//!
//! **128-channel:** 5% electrode system (Oostenveld)
//!
//! **256-channel:** Equidistant hexagonal layout, numbered positions
//!
//! ## Quick start
//!
//! ```no_run
//! use antneuro::prelude::*;
//!
//! fn main() -> anyhow::Result<()> {
//!     let sdk = AntNeuroSdk::new("lib/libeego-SDK.so")?;
//!     let amp = sdk.open_first_amplifier()?;
//!     let stream = amp.open_eeg_stream_default(500)?;
//!
//!     loop {
//!         if let Some((ch_count, sample_count, samples)) = stream.get_data()? {
//!             println!("{sample_count} samples across {ch_count} channels");
//!         }
//!     }
//! }
//! ```
//!
//! ## Async event-driven client (like muse-rs)
//!
//! ```no_run
//! use antneuro::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let client = AntNeuroClient::new(antneuro::client::AntNeuroConfig::default());
//!     let (mut rx, handle) = client.start()?;
//!
//!     while let Some(event) = rx.recv().await {
//!         match event {
//!             AntNeuroEvent::Eeg(data) => {
//!                 println!("EEG: {} ch x {} samples", data.channel_count, data.sample_count);
//!             }
//!             AntNeuroEvent::Disconnected => break,
//!             _ => {}
//!         }
//!     }
//!     handle.disconnect();
//!     Ok(())
//! }
//! ```
//!
//! ## Module overview
//!
//! | Module | Purpose |
//! |---|---|
//! | [`prelude`] | One-line glob import of the most commonly needed types |
//! | [`sdk`] | Library loading and amplifier discovery |
//! | [`amplifier`] | Amplifier control, channel queries, stream creation |
//! | [`stream`] | Data acquisition from active EEG/impedance streams |
//! | [`channel`] | Channel types and descriptors |
//! | [`client`] | High-level async event-driven client with pause/resume/disconnect |
//! | [`types`] | All event and data types |
//! | [`recording`] | CSV recording of stream data |
//! | [`error`] | Error types and SDK error code mapping |
//! | [`ffi`] | Raw FFI bindings to the eego C SDK via libloading |

pub mod amplifier;
pub mod async_backend;
pub mod backend;
pub mod channel;
pub mod client;
pub mod error;
pub mod protocol;
pub mod recording;
pub mod simulator;
pub mod sdk;
pub mod stream;
pub mod types;

#[cfg(feature = "ffi")]
pub mod ffi;
#[cfg(feature = "ffi")]
pub mod ffi_backend;

#[cfg(feature = "native")]
pub mod native;
#[cfg(feature = "native")]
pub mod usb;

// ── Prelude ───────────────────────────────────────────────────────────────────

/// Convenience re-exports for downstream crates.
///
/// A single glob import covers the entire surface area needed to discover,
/// connect, and stream data from an ANT Neuro amplifier:
///
/// ```no_run
/// use antneuro::prelude::*;
/// ```
/// Convenience re-exports for common usage.
///
/// ```rust,ignore
/// use antneuro::prelude::*;
/// ```
pub mod prelude {
    // ── Backend trait + async wrapper ────────────────────────────────────────
    pub use crate::async_backend::AsyncBackend;
    pub use crate::backend::{Backend, TriggerOutConfig};

    // ── SDK & Client ────────────────────────────────────────────────────────
    pub use crate::client::{AntNeuroClient, AntNeuroConfig, AntNeuroHandle};
    pub use crate::sdk::AntNeuroSdk;

    // ── Backend implementations ─────────────────────────────────────────────
    #[cfg(feature = "ffi")]
    pub use crate::ffi_backend::FfiBackend;
    #[cfg(feature = "native")]
    pub use crate::native::NativeBackend;

    // ── Hardware ────────────────────────────────────────────────────────────
    pub use crate::amplifier::Amplifier;
    pub use crate::channel::{Channel, ChannelType};
    pub use crate::stream::Stream;

    // ── Events and data types ───────────────────────────────────────────────
    pub use crate::types::{
        AmplifierInfo, AntNeuroEvent, EegData, ImpedanceData, PowerState, SdkVersion,
    };

    // ── Recording ───────────────────────────────────────────────────────────
    pub use crate::recording::CsvRecorder;

    // ── Errors ──────────────────────────────────────────────────────────────
    pub use crate::error::{AntNeuroError, Result};
}

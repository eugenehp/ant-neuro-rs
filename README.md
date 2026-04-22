# antneuro

Rust SDK and terminal UI for streaming EEG data from [ANT Neuro](https://www.ant-neuro.com/) eego amplifiers.

## Supported hardware

### Amplifiers

| Model | Channels (REF) | Channels (BIP) | Trigger | Max Sampling Rate | Resolution | Connection | Battery |
|---|---|---|---|---|---|---|---|
| **eego mylab** | 32, 64, 128, 256 | 24 | 8-bit TTL | 16,384 Hz | 24-bit | USB-C 3.0 | 5 h integrated |
| **eego sport 24** | 8–24 | — | 2-bit TTL | 2,048 Hz | 24-bit | USB-C 2.0 | USB-powered |
| **eego sport 32/64** | 32, 64 | — | 8-bit TTL | 16,384 Hz | 24-bit | USB-C 3.0 | 5 h integrated |
| **eego rt 8** | 8 | 24 | 2-bit TTL | 2,048 Hz | 24-bit | USB 2.0 | USB-powered |
| **eego rt 32/64** | 32, 64 | 24 | 8-bit TTL | 16,384 Hz | 24-bit | USB 2.0 | 5 h integrated |

All amplifiers: input impedance >1 GOhm, CMRR >100 dB, input noise <1.0 uVRMS.
All systems are CE Class IIa (MDR 2017/745), FDA 510(k) cleared, Health Canada licensed, ARTG registered.

### EEG caps (waveguard)

| Cap | Channels | Electrode type | Layout | Application |
|---|---|---|---|---|
| **waveguard original** | 24–256 | Ag/AgCl passive (gel) | 10-20 / equidistant | Research, clinical |
| **waveguard touch** | 32–64 | Ag/AgCl dry polymer multi-pin | 10-20 / equidistant | Rapid setup, no gel |
| **waveguard connect** | 21, 25 | Tin passive (silicone cups) | 10-20 / IFCN | Clinical routine |
| **waveguard net** | 24–256 | Saline sponge (gel-free) | 10-20 / equidistant | High-density, rapid |

### Cap-amplifier compatibility

All caps are compatible with eego mylab, eego sport, and eego rt.
The waveguard net additionally supports the eego hub.
Third-party EEG sensors can be used with eego mylab via adapter.

## Electrode positions

### 21-channel (standard 10-20)

The international 10-20 system with 19 recording electrodes plus reference and ground:

```
            Fp1   Fp2
        F7  F3  Fz  F4  F8
        T7  C3  Cz  C4  T8
        P7  P3  Pz  P4  P8
            O1      O2
```

Electrodes: **Fp1, Fp2, F3, F4, C3, C4, P3, P4, O1, O2, F7, F8, T7, T8, P7, P8, Fz, Cz, Pz** + reference (CPz) + ground (AFz)

### 25-channel (IFCN extended 10-20)

Standard 10-20 plus six inferior temporal electrodes recommended by the International Federation of Clinical Neurophysiology:

Additional positions: **F9, F10, T9, T10, P9, P10**

### 32-channel (extended 10-20)

Standard 10-20 plus additional positions from the 10-10 system:

```
                Fp1   Fp2
        F7  F3    Fz    F4  F8
    FT7 FC3  FC1  FCz  FC2  FC4 FT8
        T7  C3    Cz    C4  T8
    TP7 CP3  CP1  CPz  CP2  CP4 TP8
        P7  P3    Pz    P4  P8
            PO3   POz   PO4
                O1  Oz  O2
```

Electrodes (32): **Fp1, Fp2, F7, F3, Fz, F4, F8, FT7, FC3, FC1, FCz, FC2, FC4, FT8, T7, C3, Cz, C4, T8, TP7, CP3, CP1, CPz, CP2, CP4, TP8, P7, P3, Pz, P4, P8, O1, Oz, O2** (minus reference/ground depending on montage)

### 64-channel (10-10 system)

Full 10-10 system positions. Adds intermediate sites at every 10% interval:

**Frontal:** Fp1, Fpz, Fp2, AF7, AF3, AFz, AF4, AF8, F9, F7, F5, F3, F1, Fz, F2, F4, F6, F8, F10

**Frontocentral:** FT9, FT7, FC5, FC3, FC1, FCz, FC2, FC4, FC6, FT8, FT10

**Central:** T9, T7, C5, C3, C1, Cz, C2, C4, C6, T8, T10

**Centroparietal:** TP9, TP7, CP5, CP3, CP1, CPz, CP2, CP4, CP6, TP8, TP10

**Parietal:** P9, P7, P5, P3, P1, Pz, P2, P4, P6, P8, P10

**Parieto-occipital:** PO7, PO3, POz, PO4, PO8

**Occipital:** O1, Oz, O2, Iz

Naming convention: odd numbers = left hemisphere, even numbers = right hemisphere, `z` = midline.

### 128-channel (5% system)

Uses the Oostenveld 5% electrode system, placing electrodes at 5% intervals across the scalp. Positions include all 10-10 sites plus intermediate labels such as:

**AFF**, **FFC**, **FCC**, **CCP**, **CPP**, **PPO** rows between the standard 10-10 coronal lines.

### 256-channel (equidistant)

Uses an equidistant hexagonal layout for uniform whole-scalp coverage, optimized for high-density source reconstruction. Positions are numbered rather than named.

### Channel types in the eego SDK

| Channel type | Description | Data unit |
|---|---|---|
| `Reference` | Standard referential EEG | Volts |
| `Bipolar` | Differential between two electrodes | Volts |
| `Trigger` | External TTL trigger input | Digital |
| `SampleCounter` | Monotonic sample counter per block | Count |
| `Accelerometer` | 3-axis acceleration (IMU) | g |
| `Gyroscope` | 3-axis angular velocity (IMU) | deg/s |
| `Magnetometer` | 3-axis magnetic field (IMU) | uT |
| `ImpedanceReference` | Electrode impedance (reference) | Ohms |
| `ImpedanceGround` | Electrode impedance (ground) | Ohms |

## Platform support

| Platform | SDK streaming | USB discovery | TUI simulator |
|---|---|---|---|
| **Linux x86_64** | Full (via `libeego-SDK.so`) | Yes (`rusb`) | Yes |
| **Windows x64** | Full (via `eego-SDK.dll`) | Yes (`rusb`) | Yes |
| **Windows x86** | Full (via `eego-SDK32.dll`) | Yes (`rusb`) | Yes |
| **macOS (Apple Silicon / Intel)** | Not yet (no `.dylib` from vendor) | Yes (`rusb`) | Yes |

### macOS support

ANT Neuro does not ship a macOS native SDK library. However, this crate provides:

1. **USB device discovery** — `--usb-scan` detects eego amplifiers on macOS via libusb
2. **USB descriptor dump** — `--usb-dump` shows full USB interface/endpoint layout for protocol analysis
3. **TUI simulator** — `--simulate` runs the full waveform display without hardware
4. **Full library compilation** — all Rust code compiles on macOS; only the native `.so`/`.dll` FFI calls require Linux/Windows

The eego amplifiers communicate over USB (VID `0x2a56`, PID `0xee01`) using bulk and isochronous transfers via a proprietary protocol. The native SDK binary (`libeego-SDK.so`) implements this protocol using libusb internally. A future version of this crate aims to implement the USB protocol natively in Rust, enabling full streaming on macOS.

**macOS prerequisites:**
```bash
# Install libusb (required for USB discovery)
brew install libusb
```

## Installation

### 1. Install system dependencies

```bash
# Linux (Debian/Ubuntu)
sudo apt install libusb-1.0-0-dev

# macOS
brew install libusb

# Windows — libusb is bundled via rusb; no extra install needed.
```

### 2. Download the vendor SDK (optional)

The vendor SDK (`libeego-SDK.so` / `eego-SDK.dll`) is only needed if you
want to use the `FfiBackend`. The `NativeBackend` (pure Rust) works without it.

The binaries are publicly available in the
[BrainFlow](https://github.com/brainflow-dev/brainflow) repository under
`third_party/ant_neuro/`. The download script fetches them automatically:

```bash
# Linux / macOS:
./scripts/download-sdk.sh

# Windows (PowerShell):
.\scripts\download-sdk.ps1

# Pin to a specific BrainFlow tag:
EEGO_SDK_BRANCH=v5.12.0 ./scripts/download-sdk.sh       # bash
$env:EEGO_SDK_BRANCH="v5.12.0"; .\scripts\download-sdk.ps1  # PowerShell
```

Both scripts verify file integrity with SHA-256 checksums. Set
`EEGO_SDK_SKIP_HASH=1` (bash) or `-SkipHash` (PowerShell) to bypass.

### 3. Build

```bash
# Full build — both backends + TUI (default)
cargo build --release

# Native backend only — no vendor library needed, no TUI
cargo build --release --no-default-features --features native

# FFI backend only — requires vendor .so/.dll in lib/
cargo build --release --no-default-features --features ffi

# Library only — no binaries, no TUI
cargo build --release --lib
```

### 4. Run

```bash
# Stream EEG using the native USB backend (no vendor library needed)
cargo run --release -- --rate 1000

# Stream using the vendor SDK
cargo run --release -- --lib lib/libeego-SDK.so --rate 1000

# Launch the TUI waveform viewer
cargo run --release --bin tui

# Simulate (no hardware)
cargo run --release --bin tui -- --simulate

# List connected devices
cargo run --release -- --list

# Check USB connectivity (works on macOS too)
cargo run --release -- --usb-scan
```

### As a dependency

```toml
[dependencies]
# Both backends + TUI:
antneuro = "0.1.0"

# Native backend only (no vendor library, no TUI):
antneuro = { version = "0.1.0", default-features = false, features = ["native"] }

# FFI backend only:
antneuro = { version = "0.1.0", default-features = false, features = ["ffi"] }
```

### Feature flags

| Feature | Default | What it enables |
|---------|---------|-----------------|
| `native` | ✓ | Pure-Rust USB backend via rusb. No vendor library needed. |
| `ffi` | ✓ | Vendor SDK wrapper via libloading. Requires `.so`/`.dll` at runtime. |
| `tui` | ✓ | Terminal waveform viewer (ratatui + crossterm). |
| `download-sdk` | — | Auto-download vendor SDK from BrainFlow at build time. |

```bash
# Build with auto-download of vendor SDK:
cargo build --release --features download-sdk
```

## Quick start

### Pure Rust — no vendor library needed

```rust
use antneuro::prelude::*;

fn main() -> anyhow::Result<()> {
    let backend = NativeBackend::new()?;
    let amps = backend.get_amplifiers_info()?;
    let amp = &amps[0];
    println!("Found {} ({})", amp.serial, backend.get_amplifier_type(amp.id)?);

    backend.open_amplifier(amp.id)?;
    let channels = backend.get_amplifier_channel_list(amp.id)?;
    let rates = backend.get_amplifier_sampling_rates_available(amp.id)?;
    let stream_id = backend.open_eeg_stream(amp.id, rates[0], 1.0, 4.0, &channels)?;

    let mut buf = vec![0.0f64; 8192];
    loop {
        backend.prefetch(stream_id)?;
        let bytes = backend.get_data(stream_id, &mut buf)?;
        let samples = bytes / std::mem::size_of::<f64>();
        if samples > 0 {
            println!("{samples} samples, first value: {:.2} µV", buf[0] * 1e6);
        }
    }
}
```

This uses the pure-Rust USB backend (`NativeBackend`) — works on Linux, macOS, and Windows
with just `libusb` installed. No vendor `.so` or `.dll` required.

### Async streaming with tokio

```rust
use antneuro::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let backend = AsyncBackend::new(NativeBackend::new()?);
    let amps = backend.get_amplifiers_info().await?;
    backend.open_amplifier(amps[0].id).await?;

    let channels = backend.get_amplifier_channel_list(amps[0].id).await?;
    let stream_id = backend.open_eeg_stream(amps[0].id, 1000, 1.0, 4.0, channels).await?;
    let mut rx = backend.start_data_pump(stream_id, 8192).await?;

    while let Some(samples) = rx.recv().await {
        println!("{} samples", samples.len());
    }
    Ok(())
}
```

### Using the vendor SDK (libeego-SDK.so)

```rust
use antneuro::prelude::*;

fn main() -> anyhow::Result<()> {
    let sdk = AntNeuroSdk::new("lib/libeego-SDK.so")?;

    // List all connected amplifiers
    for info in sdk.get_amplifiers_info()? {
        println!("Found: id={} serial={}", info.id, info.serial);
    }

    // Open first amplifier and start streaming
    let amp = sdk.open_first_amplifier()?;
    println!("Type: {}", amp.amplifier_type()?);
    println!("Serial: {}", amp.serial()?);
    println!("Sampling rates: {:?}", amp.sampling_rates_available()?);

    let stream = amp.open_eeg_stream_default(500)?;
    println!("Channels: {}", stream.channel_count());

    loop {
        if let Some((ch_count, sample_count, samples)) = stream.get_data()? {
            println!("{sample_count} samples x {ch_count} channels");
        }
    }
}
```

### Async event-driven client

```rust
use antneuro::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = antneuro::client::AntNeuroConfig {
        library_path: "lib/libeego-SDK.so".into(),
        sampling_rate: 1000,
        ..Default::default()
    };
    let client = antneuro::client::AntNeuroClient::new(config);
    let (mut rx, handle) = client.start()?;

    while let Some(event) = rx.recv().await {
        match event {
            AntNeuroEvent::Connected(info) => {
                println!("Connected: {}", info.serial);
            }
            AntNeuroEvent::Eeg(data) => {
                println!("EEG: {} ch x {} samples", data.channel_count, data.sample_count);
            }
            AntNeuroEvent::Disconnected => break,
            _ => {}
        }
    }
    handle.disconnect();
    Ok(())
}
```

### Record to CSV

```rust
use antneuro::prelude::*;

fn main() -> anyhow::Result<()> {
    let sdk = AntNeuroSdk::new("lib/libeego-SDK.so")?;
    let amp = sdk.open_first_amplifier()?;
    let stream = amp.open_eeg_stream_default(500)?;
    let mut recorder = CsvRecorder::new("recording.csv", stream.channels())?;

    for _ in 0..1000 {
        if let Some((ch_count, sample_count, samples)) = stream.get_data()? {
            recorder.write_block(ch_count, sample_count, &samples, 500.0)?;
        }
    }
    recorder.flush()?;
    Ok(())
}
```

## CLI usage

```bash
# List connected amplifiers with full details (requires SDK library)
antneuro --lib lib/libeego-SDK.so --list

# Stream EEG at 1000 Hz, print to stdout
antneuro --lib lib/libeego-SDK.so --rate 1000

# Record 60 seconds of EEG to CSV
antneuro --lib lib/libeego-SDK.so --rate 500 --output recording.csv --duration 60

# Check electrode impedances
antneuro --lib lib/libeego-SDK.so --impedance

# Scan for eego USB devices (works on all platforms including macOS, no SDK needed)
antneuro --usb-scan

# Dump USB descriptors for protocol analysis
antneuro --usb-dump
```

### Interactive commands (while streaming)

| Command | Action |
|---|---|
| `q` | Quit |
| `p` | Pause streaming |
| `r` | Resume streaming |
| `i` | Show amplifier info |

## TUI usage

```bash
# Real-time waveform display (requires connected amplifier)
cargo run --bin tui -- --lib lib/libeego-SDK.so

# Simulator mode (no hardware needed)
cargo run --bin tui -- --simulate

# Custom sampling rate
cargo run --bin tui -- --lib lib/libeego-SDK.so --rate 1000
```

### TUI keyboard shortcuts

| Key | Action |
|---|---|
| `Tab` | Open device picker |
| `+` / `-` | Zoom Y-axis out / in |
| `a` | Auto-scale to peak amplitude |
| `v` | Toggle smooth overlay (9-pt moving average) |
| `p` | Pause streaming |
| `r` | Resume streaming |
| `c` | Clear waveform buffers |
| `d` | Disconnect current amplifier |
| `q` / `Esc` | Quit |

### Device picker (Tab overlay)

| Key | Action |
|---|---|
| `Up` / `Down` | Navigate amplifier list |
| `Enter` | Connect to selected amplifier |
| `Esc` | Close picker |

## Architecture

There are **two backends** that implement the same `Backend` trait:

| Backend | Requires | Platforms | Use case |
|---------|----------|-----------|----------|
| `NativeBackend` | `libusb` only | Linux, macOS, Windows | Pure Rust, no vendor library |
| `FfiBackend` | `libeego-SDK.so` / `.dll` | Linux, Windows | Wraps the vendor's C SDK |

```
antneuro/
├── Cargo.toml
├── scripts/
│   ├── download-sdk.sh           # Download vendor SDK (bash — Linux/macOS/WSL)
│   └── download-sdk.ps1          # Download vendor SDK (PowerShell — Windows)
├── lib/                          # Vendor SDK shared libraries (optional)
│   └── libeego-SDK.so
├── shim/                         # LD_PRELOAD virtual-device shim (testing)
│   ├── libusb_shim.c            # USB call interceptor + JSONL logger
│   └── vusb.c                   # Virtual eego device (SSP frame generator)
└── src/
    ├── lib.rs                    # Public API + prelude
    ├── backend.rs                # Backend trait + AsyncBackend wrapper
    ├── native_backend.rs         # Pure-Rust USB backend (SSP parser, streaming)
    ├── ffi_backend.rs            # Vendor SDK wrapper via libloading
    ├── protocol.rs               # USB protocol constants + command table
    ├── channel.rs                # Channel types (Reference, Bipolar, Trigger, ...)
    ├── types.rs                  # AmplifierInfo, PowerState, EegData, events
    ├── error.rs                  # Error types (thiserror) + SDK error codes
    ├── client.rs                 # Async event-driven client (tokio)
    ├── sdk.rs                    # High-level SDK wrapper
    ├── amplifier.rs              # Amplifier handle (wraps Backend methods)
    ├── stream.rs                 # Stream handle (prefetch + get_data)
    ├── recording.rs              # CSV recorder
    ├── simulator.rs              # Synthetic waveform generator
    └── usb.rs                    # USB device discovery (rusb)
```

### Data pipeline

```
Load SDK library (dlopen)
    │
    ▼
Discover amplifiers (get_amplifiers_info)
    │
    ▼
Open amplifier (open_amplifier)
    │
    ├── Query capabilities
    │   ├── channel_list()
    │   ├── sampling_rates_available()
    │   ├── reference_ranges_available()
    │   └── bipolar_ranges_available()
    │
    ▼
Open stream (open_eeg_stream / open_impedance_stream)
    │
    ▼
Poll loop:
    ├── prefetch() → bytes available
    ├── get_data() → buffer of f64 (Volts / Ohms)
    └── emit AntNeuroEvent via tokio mpsc channel
    │
    ▼
Close stream → Close amplifier → SDK exit
```

## Available sampling rates

Depending on amplifier model:

| Rate (Hz) | Period (ms) | Use case |
|---|---|---|
| 500 | 2.0 | Standard clinical EEG |
| 512 | 1.95 | Standard research EEG |
| 1,000 | 1.0 | Event-related potentials |
| 1,024 | 0.98 | High-resolution research |
| 2,000 | 0.5 | Fast cortical dynamics |
| 2,048 | 0.49 | High-resolution BCI |
| 4,000 | 0.25 | EMG / nerve conduction |
| 4,096 | 0.24 | EMG / nerve conduction |
| 8,000 | 0.125 | Ultra-fast acquisition |
| 8,192 | 0.122 | Ultra-fast acquisition |
| 16,000 | 0.0625 | Maximum rate (mylab, sport 32/64, rt 32/64) |
| 16,384 | 0.061 | Maximum rate (mylab, sport 32/64, rt 32/64) |

## Reference voltage ranges

| Range (V) | Use case |
|---|---|
| 1.0 | Default, standard EEG |
| 0.75 | Higher gain |
| 0.15 | Maximum sensitivity |

## Bipolar voltage ranges

| Range (V) | Use case |
|---|---|
| 4.0 | Default, wide range |
| 1.5 | Moderate sensitivity |
| 0.7 | High sensitivity |
| 0.35 | Maximum sensitivity |

## License

MIT

## Related

- [BrainFlow](https://github.com/brainflow-dev/brainflow) — universal brain-computer interface library (includes ANT Neuro board support)
- [muse-rs](https://github.com/eugenehp/muse-rs) — Rust client for Muse EEG headsets over BLE (sister project, same architecture)
- [ANT Neuro eego SDK](https://www.ant-neuro.com/) — official C/C++ SDK (wrapped by this crate)

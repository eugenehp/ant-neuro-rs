//! Async wrapper around any sync [`Backend`].
//!
//! Offloads blocking calls to the tokio thread pool and provides a
//! `tokio::sync::mpsc` channel for streaming data.

use std::sync::Arc;

use crate::backend::Backend;
use crate::channel::Channel;
use crate::error::{AntNeuroError, Result};
use crate::types::AmplifierInfo;

/// Run a closure on the tokio blocking pool with an `Arc<dyn Backend>`.
macro_rules! blocking {
    ($self:expr, |$b:ident| $body:expr) => {{
        let $b = Arc::clone(&$self.inner);
        tokio::task::spawn_blocking(move || $body)
            .await
            .map_err(|_| AntNeuroError::InternalError)?
    }};
}

/// Async adapter that wraps any sync [`Backend`].
///
/// ```rust,ignore
/// let backend = AsyncBackend::new(NativeBackend::new()?);
/// let info = backend.get_amplifiers_info().await?;
/// backend.open_amplifier(info[0].id).await?;
///
/// let mut rx = backend.start_data_pump(stream_id, 8192).await?;
/// while let Some(samples) = rx.recv().await {
///     println!("{} samples", samples.len());
/// }
/// ```
pub struct AsyncBackend {
    inner: Arc<dyn Backend>,
}

impl AsyncBackend {
    pub fn new(backend: impl Backend + 'static) -> Self {
        Self { inner: Arc::new(backend) }
    }

    pub fn inner(&self) -> &dyn Backend {
        &*self.inner
    }

    pub async fn get_version(&self) -> i32 {
        self.inner.get_version()
    }

    pub async fn get_amplifiers_info(&self) -> Result<Vec<AmplifierInfo>> {
        blocking!(self, |b| b.get_amplifiers_info())
    }

    pub async fn open_amplifier(&self, id: i32) -> Result<()> {
        blocking!(self, |b| b.open_amplifier(id))
    }

    pub async fn close_amplifier(&self, id: i32) -> Result<()> {
        blocking!(self, |b| b.close_amplifier(id))
    }

    pub async fn get_amplifier_serial(&self, id: i32) -> Result<String> {
        blocking!(self, |b| b.get_amplifier_serial(id))
    }

    pub async fn get_amplifier_channel_list(&self, id: i32) -> Result<Vec<Channel>> {
        blocking!(self, |b| b.get_amplifier_channel_list(id))
    }

    pub async fn get_amplifier_sampling_rates_available(&self, id: i32) -> Result<Vec<i32>> {
        blocking!(self, |b| b.get_amplifier_sampling_rates_available(id))
    }

    pub async fn open_eeg_stream(
        &self,
        amp: i32,
        rate: i32,
        ref_range: f64,
        bip_range: f64,
        channels: Vec<Channel>,
    ) -> Result<i32> {
        blocking!(self, |b| b.open_eeg_stream(amp, rate, ref_range, bip_range, &channels))
    }

    pub async fn close_stream(&self, id: i32) -> Result<()> {
        blocking!(self, |b| b.close_stream(id))
    }

    /// Stream samples to a tokio channel. Runs until the receiver drops.
    pub async fn start_data_pump(
        &self,
        stream_id: i32,
        buf_size: usize,
    ) -> Result<tokio::sync::mpsc::Receiver<Vec<f64>>> {
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let b = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let mut buf = vec![0.0f64; buf_size];
            while !tx.is_closed() {
                let _ = b.prefetch(stream_id);
                match b.get_data(stream_id, &mut buf) {
                    Ok(bytes) => {
                        let n = bytes / std::mem::size_of::<f64>();
                        if n > 0 && tx.blocking_send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(1)),
                }
            }
        });
        Ok(rx)
    }
}

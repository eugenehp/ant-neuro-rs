//! `App` struct and `AppMode` enum -- core TUI state.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::helpers::{DEFAULT_SCALE, MAX_DISPLAY_CH, WINDOW_SECS, Y_SCALES};

// ── App mode ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum AppMode {
    Scanning,
    Connecting(String),
    Connected { serial: String },
    Simulated,
    NoDevices,
    Disconnected,
}

// ── App state ─────────────────────────────────────────────────────────────────

pub struct App {
    pub(crate) bufs: Vec<VecDeque<f64>>,
    pub(crate) num_channels: usize,
    pub(crate) sample_rate: f64,
    pub(crate) buf_size: usize,
    pub mode: AppMode,
    pub battery_level: Option<i32>,
    pub is_charging: Option<bool>,
    pub(crate) total_samples: u64,
    pub(crate) pkt_times: VecDeque<Instant>,
    pub(crate) scale_idx: usize,
    pub paused: bool,
    pub show_picker: bool,
    pub picker_cursor: usize,
    pub picker_entries: Vec<String>,
    pub picker_connected_idx: Option<usize>,
    pub picker_scanning: bool,
    pub last_error: Option<String>,
    pub smooth: bool,
    pub(crate) channel_labels: Vec<String>,
}

impl App {
    pub(crate) fn new(num_channels: usize, sample_rate: f64) -> Self {
        let buf_size = (WINDOW_SECS * sample_rate) as usize;
        Self {
            bufs: (0..num_channels)
                .map(|_| VecDeque::with_capacity(buf_size + 16))
                .collect(),
            num_channels,
            sample_rate,
            buf_size,
            mode: AppMode::Scanning,
            battery_level: None,
            is_charging: None,
            total_samples: 0,
            pkt_times: VecDeque::with_capacity(256),
            scale_idx: DEFAULT_SCALE,
            paused: false,
            show_picker: false,
            picker_cursor: 0,
            picker_entries: vec![],
            picker_connected_idx: None,
            picker_scanning: false,
            last_error: None,
            smooth: true,
            channel_labels: (0..num_channels)
                .map(|i| format!("CH{}", i))
                .collect(),
        }
    }

    pub(crate) fn set_channel_labels(&mut self, labels: Vec<String>) {
        self.channel_labels = labels;
    }

    pub(crate) fn ensure_channels(&mut self, n: usize) {
        while self.bufs.len() < n {
            self.bufs
                .push(VecDeque::with_capacity(self.buf_size + 16));
            self.channel_labels.push(format!("CH{}", self.bufs.len() - 1));
        }
        self.num_channels = self.num_channels.max(n);
    }

    pub fn push(&mut self, ch: usize, samples: &[f64]) {
        if self.paused {
            return;
        }
        self.ensure_channels(ch + 1);
        let buf = &mut self.bufs[ch];
        for &v in samples {
            buf.push_back(v);
            while buf.len() > self.buf_size {
                buf.pop_front();
            }
        }
        if ch == 0 {
            self.total_samples += samples.len() as u64;
            let now = Instant::now();
            self.pkt_times.push_back(now);
            while self
                .pkt_times
                .front()
                .map(|t| now.duration_since(*t) > Duration::from_secs(2))
                .unwrap_or(false)
            {
                self.pkt_times.pop_front();
            }
        }
    }

    pub fn clear(&mut self) {
        for b in &mut self.bufs {
            b.clear();
        }
        self.total_samples = 0;
        self.pkt_times.clear();
        self.battery_level = None;
        self.is_charging = None;
        self.last_error = None;
    }

    pub(crate) fn pkt_rate(&self) -> f64 {
        let n = self.pkt_times.len();
        if n < 2 {
            return 0.0;
        }
        let span = self
            .pkt_times
            .back()
            .unwrap()
            .duration_since(self.pkt_times[0])
            .as_secs_f64();
        if span < 1e-9 {
            0.0
        } else {
            (n as f64 - 1.0) / span
        }
    }

    pub(crate) fn y_range(&self) -> f64 {
        Y_SCALES[self.scale_idx]
    }

    pub(crate) fn scale_up(&mut self) {
        if self.scale_idx + 1 < Y_SCALES.len() {
            self.scale_idx += 1;
        }
    }

    pub(crate) fn scale_down(&mut self) {
        if self.scale_idx > 0 {
            self.scale_idx -= 1;
        }
    }

    pub(crate) fn auto_scale(&mut self) {
        let peak = self
            .bufs
            .iter()
            .flat_map(|b| b.iter())
            .fold(0.0_f64, |acc, &v| acc.max(v.abs()));
        let needed = peak * 1.1;
        self.scale_idx = Y_SCALES
            .iter()
            .position(|&s| s >= needed)
            .unwrap_or(Y_SCALES.len() - 1);
    }

    pub(crate) fn display_channels(&self) -> usize {
        self.num_channels.min(MAX_DISPLAY_CH)
    }
}

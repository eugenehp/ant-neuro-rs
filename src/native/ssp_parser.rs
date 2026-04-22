// ── SSP (Serial Streaming Protocol) Frame Parser ─────────────────────────────
//
// SSP is ANT Neuro's framing protocol for isochronous USB data. The eego
// hardware emits a continuous byte stream of SSP frames on the isochronous
// endpoint. Each frame carries one sample row plus metadata and a sync
// trailer. This parser ring-buffers partial frames across USB reads and
// emits complete samples as f64 values.
//
// Frame layout (eego family, 408 bytes):
//
//   offset  size  field
//   ------  ----  -----
//   0x000   2     reserved
//   0x002   2     seq (u16 LE, frame counter)
//   0x004   12    reserved
//   0x010   96    24 x i32 LE channel samples
//   0x070   268   reserved / extended channel data
//   0x17c   1     mode byte (1=Idle, 2=Streaming, 3=Cal, 5=Impedance)
//   0x17d   3     reserved
//   0x180   4     trigger / aux u32
//   0x184   16    reserved
//   0x194   4     sync trailer: A5 A5 A5 A5
//
// The i32 samples are raw ADC counts. Scaling to voltage is:
//   voltage = (sample as f64) * (ref_range / 2^23)
// Currently we emit raw i32 -> f64 cast (no scaling) so both backends
// use the same units.

use crate::protocol::DeviceFamily;

// Per-family sync trailer bytes.
const SYNC_A5: [u8; 4] = [0xA5, 0xA5, 0xA5, 0xA5];
const SYNC_C5: [u8; 4] = [0xC5, 0xC5, 0xC5, 0xC5];

/// Trait for family-specific SSP frame decoders.
#[allow(dead_code)]
pub(crate) trait DataParser: Send {
    /// Parse raw USB bytes into f64 samples appended to `output`.
    /// Returns the number of complete frames decoded.
    fn parse(&mut self, raw: &[u8], channel_count: usize, output: &mut Vec<f64>) -> usize;

    /// Reset internal parser state (e.g. between streams).
    fn reset(&mut self);
}

// ── Family-aware SSP parser ──────────────────────────────────────────

/// Stateful SSP frame parser that handles all eego device families.
pub(crate) struct SspParser {
    family: DeviceFamily,
    ring: Vec<u8>,
    write_pos: usize,
    prev_seq: u16,
    total_samples: u64,
}

impl SspParser {
    fn new(family: DeviceFamily) -> Self {
        Self {
            family,
            ring: Vec::with_capacity(4096),
            write_pos: 0,
            prev_seq: 0,
            total_samples: 0,
        }
    }

    /// Return the frame size in bytes for this family and channel count.
    fn frame_size(&self, channel_count: usize) -> usize {
        match self.family {
            DeviceFamily::Eego => 0x198,           // 408 bytes
            DeviceFamily::Eego24 => 0x44,          // 68 bytes
            DeviceFamily::EegoMini => {             // ch*28 + 10
                let ch = channel_count.min(17);
                ch * 28 + 10
            }
            _ => 0x198,
        }
    }

    /// Try to decode one complete frame from the ring. Returns true if a frame was consumed.
    fn try_consume_frame(&mut self, channel_count: usize, output: &mut Vec<f64>) -> bool {
        let fsz = self.frame_size(channel_count);
        if fsz == 0 || self.write_pos < fsz {
            return false;
        }

        match self.family {
            DeviceFamily::EegoMini => self.consume_eegomini(channel_count, fsz, output),
            DeviceFamily::Eego24 => self.consume_eego24(channel_count, fsz, output),
            _ => self.consume_eego(channel_count, fsz, output),
        }
    }

    /// Decode one standard eego frame (24 x i32 channels, A5 sync trailer).
    fn consume_eego(&mut self, channel_count: usize, fsz: usize, output: &mut Vec<f64>) -> bool {
        if self.ring[0x194..0x198] != SYNC_A5 {
            // Not aligned -- skip one byte and retry.
            self.ring.copy_within(1..self.write_pos, 0);
            self.write_pos -= 1;
            return false;
        }
        let seq = u16::from_le_bytes([self.ring[0x002], self.ring[0x003]]);
        self.prev_seq = seq;
        self.total_samples += 1;
        let n_ch = channel_count.min(24);
        for c in 0..n_ch {
            let off = 0x010 + c * 4;
            let v = i32::from_le_bytes([self.ring[off], self.ring[off+1], self.ring[off+2], self.ring[off+3]]);
            output.push(v as f64);
        }
        for _ in n_ch..channel_count { output.push(0.0); }
        self.advance(fsz);
        true
    }

    /// Decode one eegomini frame (7 x i16 samples per channel, A5 sync trailer).
    fn consume_eegomini(&mut self, channel_count: usize, fsz: usize, output: &mut Vec<f64>) -> bool {
        let ch = channel_count.min(17);
        let tail = ch * 28;
        if self.ring[tail+6..tail+10] != SYNC_A5 {
            self.ring.copy_within(1..self.write_pos, 0);
            self.write_pos -= 1;
            return false;
        }
        let seq = u16::from_le_bytes([self.ring[tail], self.ring[tail+1]]);
        self.prev_seq = seq;
        self.total_samples += 1;
        // 7 interleaved i16 samples per channel.
        for c in 0..ch {
            for s in 0..7 {
                let off = c * 28 + s * 2;
                let v = i16::from_le_bytes([self.ring[off], self.ring[off+1]]);
                output.push(v as f64);
            }
        }
        self.advance(fsz);
        true
    }

    /// Decode one eego24 frame (8 x f32 channels, C5 sync trailer).
    fn consume_eego24(&mut self, channel_count: usize, fsz: usize, output: &mut Vec<f64>) -> bool {
        if self.ring[0x40..0x44] != SYNC_C5 {
            self.ring.copy_within(1..self.write_pos, 0);
            self.write_pos -= 1;
            return false;
        }
        let seq = u16::from_le_bytes([self.ring[0x06], self.ring[0x07]]);
        self.prev_seq = seq;
        self.total_samples += 1;
        let n_ch = channel_count.min(8);
        for c in 0..n_ch {
            let off = 0x10 + c * 4;
            let v = f32::from_le_bytes([self.ring[off], self.ring[off+1], self.ring[off+2], self.ring[off+3]]);
            output.push(v as f64);
        }
        for _ in n_ch..channel_count { output.push(0.0); }
        self.advance(fsz);
        true
    }

    /// Remove `n` consumed bytes from the front of the ring.
    fn advance(&mut self, n: usize) {
        let remaining = self.write_pos - n;
        if remaining > 0 {
            self.ring.copy_within(n..self.write_pos, 0);
        }
        self.write_pos = remaining;
    }
}

impl DataParser for SspParser {
    fn parse(&mut self, raw: &[u8], channel_count: usize, output: &mut Vec<f64>) -> usize {
        if channel_count == 0 || raw.is_empty() { return 0; }
        let needed = self.write_pos + raw.len();
        if needed > self.ring.len() { self.ring.resize(needed, 0); }
        self.ring[self.write_pos..self.write_pos + raw.len()].copy_from_slice(raw);
        self.write_pos += raw.len();
        // Pre-reserve output capacity to avoid realloc during frame decoding.
        let fsz = self.frame_size(channel_count).max(1);
        let est_frames = self.write_pos / fsz;
        output.reserve(est_frames * channel_count);
        let mut samples = 0usize;
        while self.try_consume_frame(channel_count, output) { samples += 1; }
        samples
    }

    fn reset(&mut self) {
        self.write_pos = 0;
        self.prev_seq = 0;
        self.total_samples = 0;
    }
}

/// Create the appropriate SSP parser for the given device family.
pub(crate) fn create_parser(family: DeviceFamily) -> Box<dyn DataParser> {
    Box::new(SspParser::new(family))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_parser_eego() {
        let mut p = create_parser(DeviceFamily::Eego);
        // Just verify it can be created and reset without panic.
        p.reset();
    }

    #[test]
    fn test_create_parser_eegomini() {
        let mut p = create_parser(DeviceFamily::EegoMini);
        p.reset();
    }

    #[test]
    fn test_create_parser_eego24() {
        let mut p = create_parser(DeviceFamily::Eego24);
        p.reset();
    }

    /// Build a valid 408-byte eego frame with known channel values and A5 trailer.
    fn make_eego_frame(channel_values: &[i32; 24]) -> Vec<u8> {
        let mut frame = vec![0u8; 0x198]; // 408 bytes
        // seq at offset 0x002
        frame[0x002] = 0x01;
        frame[0x003] = 0x00;
        // 24 x i32 LE channels starting at offset 0x010
        for (i, &val) in channel_values.iter().enumerate() {
            let off = 0x010 + i * 4;
            let bytes = val.to_le_bytes();
            frame[off..off + 4].copy_from_slice(&bytes);
        }
        // A5 sync trailer at 0x194..0x198
        frame[0x194] = 0xA5;
        frame[0x195] = 0xA5;
        frame[0x196] = 0xA5;
        frame[0x197] = 0xA5;
        frame
    }

    #[test]
    fn test_parse_eego_frame() {
        let mut channels = [0i32; 24];
        for i in 0..24 {
            channels[i] = (i as i32 + 1) * 100;
        }
        let frame = make_eego_frame(&channels);
        let mut parser = create_parser(DeviceFamily::Eego);
        let mut output = Vec::new();
        let n = parser.parse(&frame, 24, &mut output);
        assert_eq!(n, 1);
        assert_eq!(output.len(), 24);
        for i in 0..24 {
            assert_eq!(output[i], ((i as i32 + 1) * 100) as f64);
        }
    }

    #[test]
    fn test_parse_eego_fewer_channels() {
        let mut channels = [0i32; 24];
        channels[0] = 42;
        channels[1] = -100;
        let frame = make_eego_frame(&channels);
        let mut parser = create_parser(DeviceFamily::Eego);
        let mut output = Vec::new();
        let n = parser.parse(&frame, 4, &mut output);
        assert_eq!(n, 1);
        assert_eq!(output.len(), 4);
        assert_eq!(output[0], 42.0);
        assert_eq!(output[1], -100.0);
    }

    /// Build a valid eego24 frame: 68 bytes, C5 sync trailer at 0x40..0x44.
    fn make_eego24_frame(channel_values: &[f32; 8]) -> Vec<u8> {
        let mut frame = vec![0u8; 0x44]; // 68 bytes
        // seq at offset 0x06..0x08
        frame[0x06] = 0x01;
        frame[0x07] = 0x00;
        // 8 x f32 channels starting at offset 0x10
        for (i, &val) in channel_values.iter().enumerate() {
            let off = 0x10 + i * 4;
            let bytes = val.to_le_bytes();
            frame[off..off + 4].copy_from_slice(&bytes);
        }
        // C5 sync trailer at 0x40..0x44
        frame[0x40] = 0xC5;
        frame[0x41] = 0xC5;
        frame[0x42] = 0xC5;
        frame[0x43] = 0xC5;
        frame
    }

    #[test]
    fn test_parse_eego24_frame() {
        let mut channels = [0.0f32; 8];
        for i in 0..8 {
            channels[i] = (i as f32 + 1.0) * 1.5;
        }
        let frame = make_eego24_frame(&channels);
        let mut parser = create_parser(DeviceFamily::Eego24);
        let mut output = Vec::new();
        let n = parser.parse(&frame, 8, &mut output);
        assert_eq!(n, 1);
        assert_eq!(output.len(), 8);
        for i in 0..8 {
            let expected = (i as f64 + 1.0) * 1.5;
            assert!((output[i] - expected).abs() < 0.01, "ch{}: {} != {}", i, output[i], expected);
        }
    }

    /// Build a valid eegomini frame for `ch` channels. Frame size = ch*28 + 10.
    fn make_eegomini_frame(ch: usize, sample_values: &[i16]) -> Vec<u8> {
        let fsz = ch * 28 + 10;
        let mut frame = vec![0u8; fsz];
        // 7 i16 samples per channel
        for c in 0..ch {
            for s in 0..7 {
                let idx = c * 7 + s;
                let val = if idx < sample_values.len() { sample_values[idx] } else { 0 };
                let off = c * 28 + s * 2;
                let bytes = val.to_le_bytes();
                frame[off..off + 2].copy_from_slice(&bytes);
            }
        }
        // seq at tail offset
        let tail = ch * 28;
        frame[tail] = 0x01;
        frame[tail + 1] = 0x00;
        // A5 sync at tail+6..tail+10
        frame[tail + 6] = 0xA5;
        frame[tail + 7] = 0xA5;
        frame[tail + 8] = 0xA5;
        frame[tail + 9] = 0xA5;
        frame
    }

    #[test]
    fn test_parse_eegomini_frame() {
        let ch = 2;
        // 2 channels * 7 samples = 14 values
        let values: Vec<i16> = (1..=14).collect();
        let frame = make_eegomini_frame(ch, &values);
        let mut parser = create_parser(DeviceFamily::EegoMini);
        let mut output = Vec::new();
        let n = parser.parse(&frame, ch, &mut output);
        assert_eq!(n, 1);
        assert_eq!(output.len(), ch * 7);
        for i in 0..14 {
            assert_eq!(output[i], (i as i16 + 1) as f64);
        }
    }

    #[test]
    fn test_partial_frame_across_two_parses() {
        let mut channels = [0i32; 24];
        channels[0] = 999;
        let frame = make_eego_frame(&channels);
        let mid = 200;
        let mut parser = create_parser(DeviceFamily::Eego);
        let mut output = Vec::new();
        // First half
        let n1 = parser.parse(&frame[..mid], 24, &mut output);
        assert_eq!(n1, 0);
        assert!(output.is_empty());
        // Second half
        let n2 = parser.parse(&frame[mid..], 24, &mut output);
        assert_eq!(n2, 1);
        assert_eq!(output.len(), 24);
        assert_eq!(output[0], 999.0);
    }

    #[test]
    fn test_bad_sync_skips() {
        // Frame with bad sync -- parser should skip bytes
        let frame = vec![0u8; 0x198];
        // No A5 trailer -- all zeros
        let mut parser = create_parser(DeviceFamily::Eego);
        let mut output = Vec::new();
        let n = parser.parse(&frame, 24, &mut output);
        assert_eq!(n, 0);
        assert!(output.is_empty());
    }

    #[test]
    fn test_empty_input() {
        let mut parser = create_parser(DeviceFamily::Eego);
        let mut output = Vec::new();
        let n = parser.parse(&[], 24, &mut output);
        assert_eq!(n, 0);
    }

    #[test]
    fn test_zero_channel_count() {
        let frame = make_eego_frame(&[0i32; 24]);
        let mut parser = create_parser(DeviceFamily::Eego);
        let mut output = Vec::new();
        let n = parser.parse(&frame, 0, &mut output);
        assert_eq!(n, 0);
    }
}

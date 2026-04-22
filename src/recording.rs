use std::path::Path;
use std::time::Instant;

use crate::channel::Channel;
use crate::error::Result;

/// Records EEG data to a CSV file.
pub struct CsvRecorder {
    writer: csv::Writer<std::fs::File>,
    start: Instant,
}

impl CsvRecorder {
    /// Create a new CSV recorder at the given path.
    /// Writes a header row with timestamp + one column per channel.
    pub fn new(path: impl AsRef<Path>, channels: &[Channel]) -> Result<Self> {
        let mut writer = csv::Writer::from_path(path)
            .map_err(|e| crate::error::AntNeuroError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        let mut header = vec!["timestamp_s".to_string()];
        for ch in channels {
            header.push(format!("{}_{}", ch.channel_type, ch.index));
        }
        writer.write_record(&header).map_err(|e| {
            crate::error::AntNeuroError::Io(std::io::Error::new(std::io::ErrorKind::Other, e))
        })?;
        Ok(Self {
            writer,
            start: Instant::now(),
        })
    }

    /// Write one block of samples. `data` is row-major: `[s0_ch0, s0_ch1, ..., s1_ch0, ...]`.
    pub fn write_block(
        &mut self,
        channel_count: usize,
        sample_count: usize,
        data: &[f64],
        sampling_rate: f64,
    ) -> Result<()> {
        let base_time = self.start.elapsed().as_secs_f64();
        let dt = 1.0 / sampling_rate;
        for s in 0..sample_count {
            let t = base_time + (s as f64) * dt;
            let mut record = vec![format!("{:.6}", t)];
            for c in 0..channel_count {
                record.push(format!("{:.9}", data[s * channel_count + c]));
            }
            self.writer.write_record(&record).map_err(|e| {
                crate::error::AntNeuroError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e,
                ))
            })?;
        }
        Ok(())
    }

    /// Flush buffered data to disk.
    pub fn flush(&mut self) -> Result<()> {
        self.writer.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::{Channel, ChannelType};
    use std::io::Read;

    fn test_channels(n: usize) -> Vec<Channel> {
        (0..n)
            .map(|i| Channel {
                index: i as u32,
                channel_type: ChannelType::Reference,
            })
            .collect()
    }

    #[test]
    fn test_csv_recorder_creation() {
        let path = "/tmp/ant_neuro_test_create.csv";
        let channels = test_channels(3);
        let mut rec = CsvRecorder::new(path, &channels).unwrap();
        rec.flush().unwrap();

        let mut content = String::new();
        std::fs::File::open(path).unwrap().read_to_string(&mut content).unwrap();
        let first_line = content.lines().next().unwrap();
        assert_eq!(first_line, "timestamp_s,REF_0,REF_1,REF_2");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_csv_recorder_write_block() {
        let path = "/tmp/ant_neuro_test_write.csv";
        let channels = test_channels(2);
        let mut rec = CsvRecorder::new(path, &channels).unwrap();
        let data = vec![1.0, 2.0, 3.0, 4.0];
        rec.write_block(2, 2, &data, 1000.0).unwrap();
        rec.flush().unwrap();

        let mut content = String::new();
        std::fs::File::open(path).unwrap().read_to_string(&mut content).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3); // header + 2 samples
        // Check that data values are present
        assert!(lines[1].contains("1.000000000"));
        assert!(lines[1].contains("2.000000000"));
        assert!(lines[2].contains("3.000000000"));
        assert!(lines[2].contains("4.000000000"));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_csv_recorder_flush() {
        let path = "/tmp/ant_neuro_test_flush.csv";
        let channels = test_channels(1);
        let mut rec = CsvRecorder::new(path, &channels).unwrap();
        rec.write_block(1, 1, &[42.0], 500.0).unwrap();
        rec.flush().unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("42.000000000"));
        std::fs::remove_file(path).ok();
    }
}

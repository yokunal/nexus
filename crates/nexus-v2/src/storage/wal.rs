use async_trait::async_trait;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use crate::errors::NexusV2Error;
use crate::traits::{CircularWalStore, WalRecord};

const MAGIC: &[u8; 4] = b"NX2W";

pub struct CircularWal {
    path: PathBuf,
    max_records: usize,
    next_sequence: u64,
}

impl CircularWal {
    pub fn new(path: impl AsRef<Path>, max_records: usize) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            max_records: if max_records == 0 { 1000 } else { max_records },
            next_sequence: 1,
        }
    }

    fn calculate_checksum(data: &[u8]) -> u32 {
        // Simple fast CRC32-like additive rolling hash for zero-dep native reliability
        let mut hash: u32 = 0x811c9dc5;
        for &byte in data {
            hash ^= byte as u32;
            hash = hash.wrapping_mul(0x01000193);
        }
        hash
    }
}

#[async_trait]
impl CircularWalStore for CircularWal {
    async fn append_frame(&mut self, event_type: u8, payload: &[u8]) -> Result<u64, NexusV2Error> {
        let seq = self.next_sequence;
        self.next_sequence += 1;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&self.path)?;

        let checksum = Self::calculate_checksum(payload);

        let mut frame = Vec::with_capacity(4 + 8 + 8 + 1 + 4 + payload.len() + 4);
        frame.extend_from_slice(MAGIC);
        frame.extend_from_slice(&seq.to_le_bytes());
        frame.extend_from_slice(&timestamp.to_le_bytes());
        frame.push(event_type);
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        frame.extend_from_slice(payload);
        frame.extend_from_slice(&checksum.to_le_bytes());

        file.write_all(&frame)?;
        file.sync_data()?;

        Ok(seq)
    }

    async fn recover(&mut self) -> Result<Vec<WalRecord>, NexusV2Error> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let mut file = File::open(&self.path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        let mut records = Vec::new();
        let mut cursor = 0;
        let mut max_seq = 0;

        while cursor + 29 <= buffer.len() {
            if &buffer[cursor..cursor + 4] != MAGIC {
                cursor += 1;
                continue;
            }

            let seq = u64::from_le_bytes(buffer[cursor + 4..cursor + 12].try_into().unwrap());
            let timestamp = u64::from_le_bytes(buffer[cursor + 12..cursor + 20].try_into().unwrap());
            let event_type = buffer[cursor + 20];
            let payload_len = u32::from_le_bytes(buffer[cursor + 21..cursor + 25].try_into().unwrap()) as usize;

            let total_frame_len = 25 + payload_len + 4;
            if cursor + total_frame_len > buffer.len() {
                // Incomplete write at end of WAL, stop replay gracefully
                break;
            }

            let payload = &buffer[cursor + 25..cursor + 25 + payload_len];
            let expected_checksum = u32::from_le_bytes(
                buffer[cursor + 25 + payload_len..cursor + total_frame_len].try_into().unwrap()
            );

            if Self::calculate_checksum(payload) == expected_checksum {
                records.push(WalRecord {
                    sequence: seq,
                    timestamp,
                    event_type,
                    payload: payload.to_vec(),
                });
                if seq > max_seq {
                    max_seq = seq;
                }
            }

            cursor += total_frame_len;
        }

        self.next_sequence = max_seq + 1;

        // If records exceed max_records, retain only the most recent ring buffer entries
        if records.len() > self.max_records {
            let overflow = records.len() - self.max_records;
            records.drain(0..overflow);
        }

        Ok(records)
    }

    async fn compact(&mut self) -> Result<(), NexusV2Error> {
        let records = self.recover().await?;
        if records.len() <= self.max_records {
            return Ok(());
        }

        let retained = &records[records.len() - self.max_records..];
        let temp_path = self.path.with_extension("tmp");
        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_path)?;

            for r in retained {
                let checksum = Self::calculate_checksum(&r.payload);
                let mut frame = Vec::new();
                frame.extend_from_slice(MAGIC);
                frame.extend_from_slice(&r.sequence.to_le_bytes());
                frame.extend_from_slice(&r.timestamp.to_le_bytes());
                frame.push(r.event_type);
                frame.extend_from_slice(&(r.payload.len() as u32).to_le_bytes());
                frame.extend_from_slice(&r.payload);
                frame.extend_from_slice(&checksum.to_le_bytes());
                file.write_all(&frame)?;
            }
            file.sync_all()?;
        }

        std::fs::rename(temp_path, &self.path)?;
        Ok(())
    }
}

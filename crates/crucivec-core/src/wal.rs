use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Result as IoResult, Seek, SeekFrom, Write};
use std::path::Path;

use bytemuck::{bytes_of, cast_slice, Pod, Zeroable};
use thiserror::Error;

/// Error types occurring during WAL serialization, deserialization, or point-in-time replay.
#[derive(Debug, Error)]
pub enum WalError {
    #[error("I/O error during WAL operation: {0}")]
    Io(#[from] std::io::Error),
    #[error("WAL file corruption: unexpected EOF or malformed record header")]
    CorruptedHeader,
    #[error("Vector byte slice length {actual} is not a multiple of 4 (f32 alignment)")]
    UnalignedVectorBytes { actual: usize },
    #[error("LSN out of order: expected monotonic increase from {current_lsn}, encountered {encountered_lsn}")]
    NonMonotonicLsn { current_lsn: u64, encountered_lsn: u64 },
}

/// Operation type indicator within the WAL header.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalOpType {
    Insert = 1,
    Upsert = 2,
    Delete = 3,
}

impl WalOpType {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            1 => Some(Self::Insert),
            2 => Some(Self::Upsert),
            3 => Some(Self::Delete),
            _ => None,
        }
    }
}

/// Strictly aligned 32-byte binary header preceding every WAL record.
/// Natural 8-byte alignment guarantees safe zero-copy deserialization via bytemuck.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct WalEntryHeader {
    /// Monotonically increasing Log Sequence Number.
    pub lsn: u64,
    /// Emission timestamp in nanoseconds since unix epoch.
    pub timestamp_ns: u64,
    /// Target document / vector identifier.
    pub id: u64,
    /// Length of raw vector payload in bytes (must be dimension * 4 for f32).
    pub payload_len: u32,
    /// Operation code: 1 = Insert, 2 = Upsert, 3 = Delete.
    pub op_type: u8,
    /// Explicit 3-byte zero padding maintaining 32-byte header boundary.
    pub _reserved: [u8; 3],
}

impl WalEntryHeader {
    pub const SIZE: usize = std::mem::size_of::<Self>();
}

/// High-throughput, append-only binary Write-Ahead Log writer.
pub struct WalWriter {
    writer: BufWriter<File>,
    current_lsn: u64,
}

impl WalWriter {
    /// Creates or opens an existing WAL file for append-only mutation logging.
    pub fn open(path: impl AsRef<Path>) -> IoResult<Self> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)?;
        let writer = BufWriter::with_capacity(128 * 1024, file);
        Ok(Self {
            writer,
            current_lsn: 0,
        })
    }

    /// Appends a mutation entry with monotonic LSN validation and zero-copy vector serialization.
    pub fn append(
        &mut self,
        lsn: u64,
        timestamp_ns: u64,
        id: u64,
        op_type: WalOpType,
        vector: &[f32],
    ) -> Result<(), WalError> {
        if lsn <= self.current_lsn && self.current_lsn != 0 {
            return Err(WalError::NonMonotonicLsn {
                current_lsn: self.current_lsn,
                encountered_lsn: lsn,
            });
        }

        let vector_bytes: &[u8] = cast_slice(vector);
        let header = WalEntryHeader {
            lsn,
            timestamp_ns,
            id,
            payload_len: vector_bytes.len() as u32,
            op_type: op_type as u8,
            _reserved: [0u8; 3],
        };

        self.writer.write_all(bytes_of(&header))?;
        if !vector_bytes.is_empty() {
            self.writer.write_all(vector_bytes)?;
        }
        self.current_lsn = lsn;
        Ok(())
    }

    /// Flushes all in-memory buffered WAL records to kernel disk storage.
    pub fn flush(&mut self) -> Result<(), WalError> {
        self.writer.flush()?;
        Ok(())
    }
}

/// Point-in-time active document state reconstructed from binary WAL replay.
#[derive(Debug, Default, Clone)]
pub struct ActiveDatasetSnapshot {
    /// Active vectors mapped by document ID: id -> vector.
    pub active_vectors: HashMap<u64, Vec<f32>>,
    /// Tombstoned IDs that were deleted prior to or at the LSN cutoff.
    pub tombstoned_ids: Vec<u64>,
    /// Highest LSN evaluated in this snapshot.
    pub cutoff_lsn: u64,
}

/// Binary WAL reader providing sequential replay and point-in-time materialization.
pub struct WalReader {
    reader: BufReader<File>,
}

impl WalReader {
    /// Opens an existing WAL file for sequential replay.
    pub fn open(path: impl AsRef<Path>) -> IoResult<Self> {
        let file = File::open(path)?;
        Ok(Self {
            reader: BufReader::with_capacity(128 * 1024, file),
        })
    }

    /// Replays mutations up to a specified cutoff LSN, rolling forward inserts and subtracting tombstones.
    pub fn reconstruct_state(&mut self, cutoff_lsn: u64) -> Result<ActiveDatasetSnapshot, WalError> {
        self.reader.seek(SeekFrom::Start(0))?;

        let mut snapshot = ActiveDatasetSnapshot {
            active_vectors: HashMap::new(),
            tombstoned_ids: Vec::new(),
            cutoff_lsn,
        };

        let mut header_buf = [0u8; WalEntryHeader::SIZE];
        while self.reader.read_exact(&mut header_buf).is_ok() {
            let header: WalEntryHeader = *bytemuck::from_bytes(&header_buf);
            if header.lsn > cutoff_lsn {
                break;
            }

            let payload_len = header.payload_len as usize;
            let mut vector_buf = vec![0u8; payload_len];
            if payload_len > 0 {
                self.reader.read_exact(&mut vector_buf)?;
            }

            let op = WalOpType::from_u8(header.op_type).ok_or(WalError::CorruptedHeader)?;
            match op {
                WalOpType::Insert | WalOpType::Upsert => {
                    if !payload_len.is_multiple_of(4) {
                        return Err(WalError::UnalignedVectorBytes { actual: payload_len });
                    }
                    let floats: &[f32] = cast_slice(&vector_buf);
                    snapshot.active_vectors.insert(header.id, floats.to_vec());
                }
                WalOpType::Delete => {
                    snapshot.active_vectors.remove(&header.id);
                    snapshot.tombstoned_ids.push(header.id);
                }
            }
        }

        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_size_and_alignment() {
        assert_eq!(std::mem::size_of::<WalEntryHeader>(), 32);
        assert_eq!(std::mem::align_of::<WalEntryHeader>(), 8);
    }

    #[test]
    fn test_wal_roundtrip_and_point_in_time_reconstruction() {
        let temp_dir = std::env::temp_dir();
        let wal_path = temp_dir.join(format!("test_wal_{}.bin", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));

        {
            let mut writer = WalWriter::open(&wal_path).expect("Failed to open WAL writer");
            writer.append(1, 1000, 42, WalOpType::Insert, &[1.0, 2.0, 3.0, 4.0]).unwrap();
            writer.append(2, 2000, 99, WalOpType::Insert, &[0.1, 0.2, 0.3, 0.4]).unwrap();
            writer.append(3, 3000, 42, WalOpType::Delete, &[]).unwrap();
            writer.append(4, 4000, 105, WalOpType::Insert, &[5.0, 6.0, 7.0, 8.0]).unwrap();
            writer.flush().unwrap();
        }

        let mut reader = WalReader::open(&wal_path).expect("Failed to open WAL reader");

        // Cutoff at LSN 2: document 42 and 99 are active
        let snap2 = reader.reconstruct_state(2).unwrap();
        assert_eq!(snap2.active_vectors.len(), 2);
        assert!(snap2.active_vectors.contains_key(&42));
        assert!(snap2.active_vectors.contains_key(&99));

        // Cutoff at LSN 3: document 42 was tombstoned
        let snap3 = reader.reconstruct_state(3).unwrap();
        assert_eq!(snap3.active_vectors.len(), 1);
        assert!(!snap3.active_vectors.contains_key(&42));
        assert!(snap3.active_vectors.contains_key(&99));
        assert_eq!(snap3.tombstoned_ids, vec![42]);

        // Cutoff at LSN 4: document 105 inserted
        let snap4 = reader.reconstruct_state(4).unwrap();
        assert_eq!(snap4.active_vectors.len(), 2);
        assert!(snap4.active_vectors.contains_key(&99));
        assert!(snap4.active_vectors.contains_key(&105));

        let _ = std::fs::remove_file(wal_path);
    }
}


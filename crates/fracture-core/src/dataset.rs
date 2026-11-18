use rand::Rng;
use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoroshiro128PlusPlus;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatasetError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid dataset format or corrupted header: {0}")]
    InvalidHeader(String),

    #[error("Dimension mismatch: expected {expected}, found {found}")]
    DimensionMismatch { expected: usize, found: usize },

    #[error("Unexpected end of file while reading vector")]
    UnexpectedEof,
}

/// Streaming reader for canonical `.fvecs` files (SIFT, GIST, Deep1M).
/// Format: Repeated records of [4-byte dimension `d` (int32)] + [`d * 4` bytes IEEE-754 floats].
pub struct FvecsReader {
    reader: BufReader<File>,
    dimension: usize,
    total_vectors: usize,
    current_index: usize,
}

impl FvecsReader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DatasetError> {
        let file = File::open(path)?;
        let file_len = file.metadata()?.len();
        let mut reader = BufReader::new(file);

        let mut dim_buf = [0u8; 4];
        reader.read_exact(&mut dim_buf).map_err(|_| {
            DatasetError::InvalidHeader("File too small to contain vector dimension".into())
        })?;
        let dimension = i32::from_le_bytes(dim_buf) as usize;

        if dimension == 0 || dimension > 16384 {
            return Err(DatasetError::InvalidHeader(format!(
                "Suspicious vector dimension: {}",
                dimension
            )));
        }

        let record_size = 4 + dimension * 4;
        if file_len % (record_size as u64) != 0 {
            return Err(DatasetError::InvalidHeader(format!(
                "File length {} not divisible by record size {}",
                file_len, record_size
            )));
        }

        let total_vectors = (file_len / (record_size as u64)) as usize;
        reader.seek(SeekFrom::Start(0))?;

        Ok(Self {
            reader,
            dimension,
            total_vectors,
            current_index: 0,
        })
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }

    pub fn total_vectors(&self) -> usize {
        self.total_vectors
    }

    /// Reads all vectors from the file into memory.
    pub fn read_all(path: impl AsRef<Path>) -> Result<Vec<Vec<f32>>, DatasetError> {
        let mut reader = Self::open(path)?;
        let mut results = Vec::with_capacity(reader.total_vectors());
        while let Some(vec) = reader.next_vector()? {
            results.push(vec);
        }
        Ok(results)
    }

    /// Reads the next vector embedding from the stream.
    pub fn next_vector(&mut self) -> Result<Option<Vec<f32>>, DatasetError> {
        if self.current_index >= self.total_vectors {
            return Ok(None);
        }

        let mut dim_buf = [0u8; 4];
        match self.reader.read_exact(&mut dim_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(DatasetError::Io(e)),
        }

        let dim = i32::from_le_bytes(dim_buf) as usize;
        if dim != self.dimension {
            return Err(DatasetError::DimensionMismatch {
                expected: self.dimension,
                found: dim,
            });
        }

        let mut raw_bytes = vec![0u8; self.dimension * 4];
        self.reader.read_exact(&mut raw_bytes)?;

        let mut vector = Vec::with_capacity(self.dimension);
        for chunk in raw_bytes.as_chunks::<4>().0 {
            let val = f32::from_le_bytes(*chunk);
            vector.push(val);
        }

        self.current_index += 1;
        Ok(Some(vector))
    }

    /// Writes a slice of vectors into canonical `.fvecs` format.
    pub fn write(path: impl AsRef<Path>, vectors: &[Vec<f32>]) -> Result<(), DatasetError> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);

        for vec in vectors {
            let dim = vec.len() as i32;
            writer.write_all(&dim.to_le_bytes())?;
            for &val in vec {
                writer.write_all(&val.to_le_bytes())?;
            }
        }
        writer.flush()?;
        Ok(())
    }
}

/// Streaming reader for canonical `.bvecs` files (BigANN, 8-bit quantized vectors).
/// Format: Repeated records of [4-byte dimension `d` (int32)] + [`d` bytes uint8].
pub struct BvecsReader {
    reader: BufReader<File>,
    dimension: usize,
    total_vectors: usize,
    current_index: usize,
}

impl BvecsReader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DatasetError> {
        let file = File::open(path)?;
        let file_len = file.metadata()?.len();
        let mut reader = BufReader::new(file);

        let mut dim_buf = [0u8; 4];
        reader.read_exact(&mut dim_buf).map_err(|_| {
            DatasetError::InvalidHeader("File too small to contain vector dimension".into())
        })?;
        let dimension = i32::from_le_bytes(dim_buf) as usize;

        let record_size = 4 + dimension;
        if file_len % (record_size as u64) != 0 {
            return Err(DatasetError::InvalidHeader(format!(
                "File length {} not divisible by record size {}",
                file_len, record_size
            )));
        }

        let total_vectors = (file_len / (record_size as u64)) as usize;
        reader.seek(SeekFrom::Start(0))?;

        Ok(Self {
            reader,
            dimension,
            total_vectors,
            current_index: 0,
        })
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }

    pub fn total_vectors(&self) -> usize {
        self.total_vectors
    }

    /// Reads the next byte vector, normalizing components to unit float vectors.
    pub fn next_vector(&mut self) -> Result<Option<Vec<f32>>, DatasetError> {
        if self.current_index >= self.total_vectors {
            return Ok(None);
        }

        let mut dim_buf = [0u8; 4];
        match self.reader.read_exact(&mut dim_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(DatasetError::Io(e)),
        }

        let dim = i32::from_le_bytes(dim_buf) as usize;
        if dim != self.dimension {
            return Err(DatasetError::DimensionMismatch {
                expected: self.dimension,
                found: dim,
            });
        }

        let mut raw_bytes = vec![0u8; self.dimension];
        self.reader.read_exact(&mut raw_bytes)?;

        let mut norm_sq = 0.0f32;
        let mut vector = Vec::with_capacity(self.dimension);
        for &b in &raw_bytes {
            let val = b as f32;
            vector.push(val);
            norm_sq += val * val;
        }

        let norm = norm_sq.sqrt().max(1e-10);
        for val in &mut vector {
            *val /= norm;
        }

        self.current_index += 1;
        Ok(Some(vector))
    }
}

/// High-throughput synthetic vector dataset generator with zero heap allocation per batch.
pub struct SyntheticDataset {
    rng: Xoroshiro128PlusPlus,
    dimension: usize,
}

impl SyntheticDataset {
    pub fn new(dimension: usize, seed: u64) -> Self {
        Self {
            rng: Xoroshiro128PlusPlus::seed_from_u64(seed),
            dimension,
        }
    }

    /// Generates a single unit-normalized vector.
    pub fn next_vector(&mut self) -> Vec<f32> {
        let mut vec = Vec::with_capacity(self.dimension);
        let mut norm_sq = 0.0f32;

        for _ in 0..self.dimension {
            let val: f32 = self.rng.gen_range(-1.0..1.0);
            vec.push(val);
            norm_sq += val * val;
        }

        let norm = norm_sq.sqrt().max(1e-10);
        for val in &mut vec {
            *val /= norm;
        }

        vec
    }

    /// Generates a batch of `count` unit-normalized vectors.
    pub fn generate_batch(&mut self, count: usize) -> Vec<Vec<f32>> {
        let mut batch = Vec::with_capacity(count);
        for _ in 0..count {
            batch.push(self.next_vector());
        }
        batch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fvecs_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_fracture.fvecs");

        let mut synth = SyntheticDataset::new(128, 42);
        let sample_vectors = synth.generate_batch(20);

        FvecsReader::write(&file_path, &sample_vectors).expect("Failed to write fvecs");

        let mut reader = FvecsReader::open(&file_path).expect("Failed to open fvecs");
        assert_eq!(reader.dimension(), 128);
        assert_eq!(reader.total_vectors(), 20);

        let mut loaded = Vec::new();
        while let Some(vec) = reader.next_vector().expect("Failed reading vector") {
            loaded.push(vec);
        }

        assert_eq!(loaded.len(), 20);
        for (orig, read) in sample_vectors.iter().zip(loaded.iter()) {
            assert_eq!(orig.len(), read.len());
            for (&a, &b) in orig.iter().zip(read.iter()) {
                assert!((a - b).abs() < 1e-6);
            }
        }

        let _ = std::fs::remove_file(file_path);
    }

    #[test]
    fn test_synthetic_normalization() {
        let mut synth = SyntheticDataset::new(64, 999);
        for _ in 0..50 {
            let vec = synth.next_vector();
            let norm_sq: f32 = vec.iter().map(|&x| x * x).sum();
            assert!((norm_sq.sqrt() - 1.0).abs() < 1e-5);
        }
    }
}

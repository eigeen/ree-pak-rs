use std::io::Read;

use byteorder::{LE, ReadBytesExt};

use crate::error::Result;
use crate::pak::ChunkCompressionType;

/// Chunk table (feature flag `FeatureFlags::CHUNK_TABLE`).
///
/// Some entries store `offset` as a chunk index (see `PakEntry::offset_is_chunk_index()`), and their `offset`
/// is an index into this table
/// (instead of a byte offset in the pak file). Each chunk expands to `block_size` bytes:
/// - `meta >> 10`: compressed byte length for this chunk
/// - low 10 bits: unknown flags (reserved / unknown)
///
/// Compression is inferred from the byte length: `compressed_len == block_size` means stored (uncompressed),
/// otherwise zstd.
#[derive(Debug, Clone)]
pub struct ChunkTable {
    block_size: u32,
    chunks: Vec<ChunkDesc>,
}

#[derive(Debug, Clone)]
pub struct ChunkDesc {
    start: u64,
    meta: u32,
}

impl ChunkTable {
    pub fn block_size(&self) -> u32 {
        self.block_size
    }

    pub fn chunks(&self) -> &[ChunkDesc] {
        &self.chunks
    }
}

impl ChunkDesc {
    pub fn start(&self) -> u64 {
        self.start
    }

    pub fn meta(&self) -> u32 {
        self.meta
    }

    pub fn compression_type(&self, block_size: u32) -> ChunkCompressionType {
        if self.compressed_len() == block_size {
            ChunkCompressionType::None
        } else {
            ChunkCompressionType::Zstd
        }
    }

    pub fn flags(&self) -> u16 {
        (self.meta & 0x03FF) as u16
    }

    pub fn compressed_len(&self) -> u32 {
        self.meta >> 10
    }
}

pub fn read_chunk_table<R>(reader: &mut R) -> Result<ChunkTable>
where
    R: Read,
{
    let block_size = reader.read_u32::<LE>()?;
    let count = reader.read_u32::<LE>()?;

    let mut start_lows = Vec::with_capacity(count as usize);
    let mut metas = Vec::with_capacity(count as usize);
    for _ in 0..count {
        start_lows.push(reader.read_u32::<LE>()?);
        metas.push(reader.read_u32::<LE>()?);
    }

    // Reconstruct 64-bit offsets from a monotonically increasing u32 (with wrap at 4GiB).
    let mut chunks = Vec::with_capacity(count as usize);
    let mut high = 0u64;
    let mut prev = start_lows.first().copied().unwrap_or(0);
    for (start_low, meta) in start_lows.into_iter().zip(metas) {
        if start_low < prev {
            high = high.wrapping_add(1u64 << 32);
        }
        let start = high | (start_low as u64);
        chunks.push(ChunkDesc { start, meta });
        prev = start_low;
    }

    Ok(ChunkTable { block_size, chunks })
}

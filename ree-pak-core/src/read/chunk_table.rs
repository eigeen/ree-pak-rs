use std::io::Read;

use byteorder::{LE, ReadBytesExt};

use crate::error::Result;

/// Chunk table (feature flag `FeatureFlags::CHUNK_TABLE`).
///
/// Some entries store `offset` as a chunk index (see `PakEntry::offset_is_chunk_index()`), and their `offset`
/// is an index into this table
/// (instead of a byte offset in the pak file). Each chunk expands to `block_size` bytes:
/// - `meta == 0x2000_0000`: raw chunk (stored uncompressed)
/// - otherwise: zstd-compressed chunk of length `(meta >> 10)` bytes
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

    pub fn is_raw(&self) -> bool {
        self.meta == 0x2000_0000
    }

    pub fn compressed_len(&self, block_size: u32) -> u32 {
        if self.is_raw() { block_size } else { self.meta >> 10 }
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

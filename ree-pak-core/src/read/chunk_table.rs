use std::io::Read;

use byteorder::{LE, ReadBytesExt};

use crate::error::Result;
use crate::pak::CompressionType;

/// Chunk table (feature flag `FeatureFlags::CHUNK_TABLE`).
///
/// Some entries store `offset` as a chunk index (see `PakEntry::offset_is_chunk_index()`), and their `offset`
/// is an index into this table
/// (instead of a byte offset in the pak file). Each chunk expands to `block_size` bytes:
/// - high 4 bits (`meta >> 28`): compression type (see `CompressionType`)
/// - remaining bits: compressed length info (currently `meta & 0x0FFF_FFFF`, interpreted as `(len << 10)`)
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
    const COMPRESSION_BITS_SHIFT: u32 = 28;
    const COMPRESSION_BITS_MASK: u32 = 0xF000_0000;
    const COMPRESSED_LEN_MASK: u32 = 0x0FFF_FFFF;

    pub fn start(&self) -> u64 {
        self.start
    }

    pub fn meta(&self) -> u32 {
        self.meta
    }

    pub fn compression_type(&self) -> Option<CompressionType> {
        let bits = self.compression_type_bits();
        CompressionType::from_u8(bits)
    }

    pub fn compression_type_bits(&self) -> u8 {
        (self.meta >> Self::COMPRESSION_BITS_SHIFT) as u8
    }

    pub fn compressed_len(&self, block_size: u32) -> Option<u32> {
        let compression = self.compression_type()?;
        let len = match compression {
            CompressionType::None => block_size,
            CompressionType::Deflate | CompressionType::Zstd => (self.meta & Self::COMPRESSED_LEN_MASK) >> 10,
        };
        Some(len)
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
        // Validate compression bits early so downstream code can treat `ChunkDesc::compression_type()` as infallible.
        let compression_bits = ((meta & ChunkDesc::COMPRESSION_BITS_MASK) >> ChunkDesc::COMPRESSION_BITS_SHIFT) as u8;
        if CompressionType::from_u8(compression_bits).is_none() {
            return Err(crate::error::PakError::InvalidChunkTable(format!(
                "unknown chunk compression type: 0x{:X}",
                compression_bits,
            )));
        }
        chunks.push(ChunkDesc { start, meta });
        prev = start_low;
    }

    Ok(ChunkTable { block_size, chunks })
}

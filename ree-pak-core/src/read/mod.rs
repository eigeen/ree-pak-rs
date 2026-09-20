//! Low-level pak metadata reader.
//!
//! If you want a single “open and read entries” handle, prefer [`crate::PakFile`].
//! This module is useful when you already have a `Read`/`Seek` implementation and only need
//! to parse the header + entry table (`read_metadata`) or use the lower-level entry reader APIs.

use std::io::{Cursor, Read};

use byteorder::{LE, ReadBytesExt};

use crate::error::Result;
use crate::pak::{self, CompressionType, FeatureFlags, PakEntry, PakHeader, PakMetadata};
use crate::spec;

pub mod archive;
pub mod chunk_table;
pub mod compressed;
pub mod entry;

#[derive(Debug, Clone, Copy)]
pub struct PakReadOptions {
    /// When `true`, reading fails if the pak header contains any feature flags not supported by this crate.
    pub strict_feature_flags: bool,
}

impl Default for PakReadOptions {
    fn default() -> Self {
        Self {
            strict_feature_flags: true,
        }
    }
}

/// Errors produced by entry payload reading (decompression / magic-based extension detection).
#[derive(Debug, thiserror::Error)]
pub enum PakReaderError {
    #[error("Failed to read raw data: {0}")]
    RawData(#[source] std::io::Error),
    #[error("Failed to decompress from {compression:?}: {source}")]
    Decompression {
        compression: CompressionType,
        #[source]
        source: std::io::Error,
    },
    #[error("Invalid compression type: {0}")]
    InvalidCompressionType(u8),
    #[error("Failed to determine file extension: {0}")]
    Extension(#[source] std::io::Error),
}

impl PakReaderError {
    /// Convert this error into a `std::io::Error` with a best-effort `ErrorKind`.
    pub fn into_io_error(self) -> std::io::Error {
        let kind = match &self {
            PakReaderError::RawData(e) => e.kind(),
            PakReaderError::Decompression { source, .. } => source.kind(),
            PakReaderError::Extension(e) => e.kind(),
            PakReaderError::InvalidCompressionType(_) => std::io::ErrorKind::InvalidData,
        };
        std::io::Error::new(kind, self)
    }
}

/// Read pak metadata (header + entry table) from the current stream position.
///
/// The input reader must be positioned at the start of the pak file.
///
/// If the pak header enables `FeatureFlags::ENTRY_ENCRYPTION`, this function will read the 128-byte key
/// and decrypt the entry table bytes before parsing.
pub fn read_metadata<R>(reader: &mut R) -> Result<PakMetadata>
where
    R: Read,
{
    read_metadata_with_options(reader, PakReadOptions::default())
}

/// Read pak metadata (header + entry table) using custom options.
///
/// See [`read_metadata`] for details.
pub fn read_metadata_with_options<R>(reader: &mut R, options: PakReadOptions) -> Result<PakMetadata>
where
    R: Read,
{
    // read header
    let spec_header = spec::Header::from_reader(reader)?;
    let mut header = PakHeader::try_from_spec_with_strict_feature_flags(spec_header, options.strict_feature_flags)?;

    // read entries
    let mut entry_table_bytes = vec![0; (header.entry_size() * header.total_files()) as usize];
    reader.read_exact(&mut entry_table_bytes)?;

    if header.feature.contains(FeatureFlags::EXTRA_U32) {
        // a unknown appended u32 value.
        let unk_u32 = reader.read_u32::<LE>()?;
        header.unk_u32_sig = unk_u32;
    }
    if header.feature.contains(FeatureFlags::EXTRA_DATA) {
        // First appears in RE9
        let mut extra = [0u8; 9];
        reader.read_exact(&mut extra)?;
        header.extra_data = extra.to_vec();
    }
    if header.feature.contains(FeatureFlags::REMAP_ENTRIES) {
        skip_entry_remaps(reader)?;
    }
    // decrypt
    if header.feature.contains(FeatureFlags::ENTRY_ENCRYPTION) {
        let mut raw_key = [0; 128];
        reader.read_exact(&mut raw_key)?;
        entry_table_bytes = pak::decrypt_pak_data(&entry_table_bytes, &raw_key);
    }
    // parse entries
    let entries = read_entries(&mut Cursor::new(&entry_table_bytes), &header)?;

    Ok(PakMetadata::new(header, entries))
}

fn skip_entry_remaps<R>(reader: &mut R) -> Result<()>
where
    R: Read,
{
    let count = reader.read_u64::<LE>()?;
    let mut entry = [0u8; 16];

    for _ in 0..count {
        reader.read_exact(&mut entry)?;
    }

    Ok(())
}

fn read_entries<R>(reader: &mut R, header: &PakHeader) -> Result<Vec<PakEntry>>
where
    R: Read,
{
    if header.major_version() == 2 && header.minor_version() == 0 {
        read_entries_v1(reader, header.total_files())
    } else {
        read_entries_v2(reader, header.total_files())
    }
}

fn read_entries_v1<R>(reader: &mut R, total_files: u32) -> Result<Vec<PakEntry>>
where
    R: Read,
{
    let mut entries = Vec::with_capacity(total_files as usize);
    for _ in 0..total_files {
        let spec_entry = spec::EntryV1::from_reader(reader)?;
        let entry = PakEntry::from(spec_entry);
        entries.push(entry);
    }

    Ok(entries)
}

fn read_entries_v2<R>(reader: &mut R, total_files: u32) -> Result<Vec<PakEntry>>
where
    R: Read,
{
    let mut entries = Vec::with_capacity(total_files as usize);
    for _ in 0..total_files {
        let spec_entry = spec::EntryV2::from_reader(reader)?;
        let entry = PakEntry::from(spec_entry);
        entries.push(entry);
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use byteorder::{LE, ReadBytesExt as _};

    use super::read_metadata;
    use crate::{pak::FeatureFlags, spec};

    const CHUNK_BLOCK_SIZE: u32 = 524_288;
    const CHUNK_COUNT: u32 = 46_110;

    fn header_bytes(feature: FeatureFlags) -> Vec<u8> {
        spec::Header {
            magic: *b"KPKA",
            major_version: 4,
            minor_version: 2,
            feature: feature.bits(),
            total_files: 0,
            hash: 0,
        }
        .into_bytes()
        .to_vec()
    }

    fn append_key_and_chunk_header(bytes: &mut Vec<u8>) {
        bytes.extend_from_slice(&[0u8; 128]);
        bytes.extend_from_slice(&CHUNK_BLOCK_SIZE.to_le_bytes());
        bytes.extend_from_slice(&CHUNK_COUNT.to_le_bytes());
    }

    fn assert_chunk_header(reader: &mut Cursor<Vec<u8>>) {
        assert_eq!(reader.read_u32::<LE>().unwrap(), CHUNK_BLOCK_SIZE);
        assert_eq!(reader.read_u32::<LE>().unwrap(), CHUNK_COUNT);
    }

    #[test]
    fn read_metadata_without_remaps_preserves_existing_layout() {
        let feature = FeatureFlags::ENTRY_ENCRYPTION | FeatureFlags::CHUNK_TABLE;
        assert_eq!(feature.bits(), 0x28);

        let mut bytes = header_bytes(feature);
        append_key_and_chunk_header(&mut bytes);

        let mut reader = Cursor::new(bytes);
        let metadata = read_metadata(&mut reader).unwrap();

        assert!(metadata.entries().is_empty());
        assert_eq!(metadata.header().feature(), feature);
        assert_chunk_header(&mut reader);
    }

    #[test]
    fn read_metadata_skips_remaps_before_encryption_key() {
        let feature = FeatureFlags::ENTRY_ENCRYPTION | FeatureFlags::CHUNK_TABLE | FeatureFlags::REMAP_ENTRIES;
        assert_eq!(feature.bits(), 0x68);
        assert_eq!(FeatureFlags::BIT06, FeatureFlags::REMAP_ENTRIES);

        let mut bytes = header_bytes(feature);
        bytes.extend_from_slice(&1u64.to_le_bytes());
        for value in [1u32, 2, 3, 4] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        append_key_and_chunk_header(&mut bytes);

        let mut reader = Cursor::new(bytes);
        let metadata = read_metadata(&mut reader).unwrap();

        assert!(metadata.entries().is_empty());
        assert_eq!(metadata.header().feature(), feature);
        assert_chunk_header(&mut reader);
    }
}

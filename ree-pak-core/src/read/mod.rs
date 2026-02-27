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
        // a unknown appended u32 value
        let unk_u32 = reader.read_u32::<LE>()?;
        header.unk_u32_sig = unk_u32;
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

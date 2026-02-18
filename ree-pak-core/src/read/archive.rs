use std::borrow::Cow;
use std::io::{Cursor, Read, Seek};

use crate::error::{PakError, Result};
use crate::pak::{PakEntry, PakMetadata};

use super::entry::PakEntryReader;

/// Read pak entries using pak metadata + an underlying reader.
///
/// Note: this reader path only supports entries whose offsets are *byte offsets*.
/// For chunk-index offsets (`FeatureFlags::CHUNK_TABLE`), use [`crate::PakFile::open_entry`].
pub struct PakMetadataReader<'a, R> {
    reader: R,
    metadata: Cow<'a, PakMetadata>,
}

impl<'a, R> PakMetadataReader<'a, R>
where
    R: Read + Seek,
{
    /// Create a reader that borrows `metadata`.
    pub fn new(reader: R, metadata: &'a PakMetadata) -> Self {
        Self {
            reader,
            metadata: Cow::Borrowed(metadata),
        }
    }

    /// Create a reader that owns `metadata`.
    pub fn new_owned(reader: R, metadata: PakMetadata) -> Self {
        Self {
            reader,
            metadata: Cow::Owned(metadata),
        }
    }

    /// Consume the wrapper and return the underlying reader.
    pub fn into_inner(self) -> R {
        self.reader
    }

    /// Access the metadata (borrowed or owned).
    pub fn metadata(&self) -> &PakMetadata {
        &self.metadata
    }

    /// Open an entry reader by providing an owned [`PakEntry`].
    ///
    /// This reads the full compressed bytes into memory and returns a reader over decompressed bytes.
    pub fn owned_entry_reader(&mut self, entry: PakEntry) -> Result<PakEntryReader<Cursor<Vec<u8>>>> {
        PakEntryReader::new_owned(&mut self.reader, entry)
    }

    /// Open an entry reader by index into the entry table.
    pub fn owned_entry_reader_by_index(&mut self, index: usize) -> Result<PakEntryReader<Cursor<Vec<u8>>>> {
        let entry = self
            .metadata
            .entries()
            .get(index)
            .ok_or(PakError::EntryIndexOutOfBounds)?;
        PakEntryReader::new_owned(&mut self.reader, entry.clone())
    }
}

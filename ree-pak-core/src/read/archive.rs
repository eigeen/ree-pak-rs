use std::borrow::Cow;
use std::io::{Cursor, Read, Seek};

use crate::error::{PakError, Result};
use crate::pak::{PakEntry, PakMetadata};

use super::entry::PakEntryReader;

/// Read pak entries using pak metadata + an underlying reader.
pub struct PakMetadataReader<'a, R> {
    reader: R,
    metadata: Cow<'a, PakMetadata>,
}

impl<'a, R> PakMetadataReader<'a, R>
where
    R: Read + Seek,
{
    pub fn new(reader: R, metadata: &'a PakMetadata) -> Self {
        Self {
            reader,
            metadata: Cow::Borrowed(metadata),
        }
    }

    pub fn new_owned(reader: R, metadata: PakMetadata) -> Self {
        Self {
            reader,
            metadata: Cow::Owned(metadata),
        }
    }

    pub fn into_inner(self) -> R {
        self.reader
    }

    pub fn metadata(&self) -> &PakMetadata {
        &self.metadata
    }

    pub fn owned_entry_reader(&mut self, entry: PakEntry) -> Result<PakEntryReader<Cursor<Vec<u8>>>> {
        PakEntryReader::new_owned(&mut self.reader, entry)
    }

    pub fn owned_entry_reader_by_index(&mut self, index: usize) -> Result<PakEntryReader<Cursor<Vec<u8>>>> {
        let entry = self
            .metadata
            .entries()
            .get(index)
            .ok_or(PakError::EntryIndexOutOfBounds)?;
        PakEntryReader::new_owned(&mut self.reader, entry.clone())
    }
}

use std::io::{Cursor, Read, Seek};

use crate::error::{PakError, Result};
use crate::pak::{PakArchive, PakEntry};

use super::entry::PakEntryReader;

/// Read a pak archive.
pub struct PakArchiveReader<'a, R> {
    reader: R,
    archive: OwnedPakArchive<'a>,
}

impl<'a, R> PakArchiveReader<'a, R>
where
    R: Read + Seek,
{
    pub fn new(reader: R, archive: &'a PakArchive) -> Self {
        Self {
            reader,
            archive: OwnedPakArchive::Borrowed(archive),
        }
    }

    pub fn new_owned(reader: R, archive: PakArchive) -> Self {
        Self {
            reader,
            archive: OwnedPakArchive::Owned(archive),
        }
    }

    pub fn into_inner(self) -> R {
        self.reader
    }

    pub fn archive(&self) -> &PakArchive {
        self.archive.inner()
    }

    pub fn owned_entry_reader(&mut self, entry: PakEntry) -> Result<PakEntryReader<Cursor<Vec<u8>>>> {
        PakEntryReader::new_owned(&mut self.reader, entry)
    }

    pub fn owned_entry_reader_by_index(&mut self, index: usize) -> Result<PakEntryReader<Cursor<Vec<u8>>>> {
        let entry = self
            .archive
            .inner()
            .entries()
            .get(index)
            .ok_or(PakError::EntryIndexOutOfBounds)?;
        PakEntryReader::new_owned(&mut self.reader, entry.clone())
    }
}

pub enum OwnedPakArchive<'a> {
    Owned(PakArchive),
    Borrowed(&'a PakArchive),
}

impl<'a> OwnedPakArchive<'a> {
    pub fn inner(&self) -> &PakArchive {
        match self {
            OwnedPakArchive::Owned(inner) => inner,
            OwnedPakArchive::Borrowed(inner) => inner,
        }
    }
}

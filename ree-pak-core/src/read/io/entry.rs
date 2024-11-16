use std::io::{BufRead, Cursor, Read, Seek, SeekFrom};

use crate::error::Result;
use crate::pak::PakEntry;

use super::compressed::CompressedReader;
use super::extension::ExtensionReader;

/// Read a pak entry file.
pub struct PakEntryReader<R> {
    reader: ExtensionReader<CompressedReader<R>>,
}

impl<R> Read for PakEntryReader<R>
where
    R: BufRead,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buf)
    }
}

impl PakEntryReader<Cursor<Vec<u8>>> {
    /// Create a new owned reader from full pak reader
    pub fn new_owned<R1>(reader: &mut R1, entry: PakEntry) -> Result<Self>
    where
        R1: Read + Seek,
    {
        reader.seek(SeekFrom::Start(entry.offset()))?;
        let mut data = vec![0; entry.compressed_size() as usize];
        reader.read_exact(&mut data)?;
        let owned_reader = Cursor::new(data);

        let compression = entry.compression_method();
        let r = ExtensionReader::new(CompressedReader::new(owned_reader, compression)?);
        Ok(Self { reader: r })
    }
}

impl<R> PakEntryReader<R>
where
    R: BufRead,
{
    pub fn from_part_reader(part_reader: R, entry: &PakEntry) -> Result<Self> {
        let compression = entry.compression_method();
        let r = ExtensionReader::new(CompressedReader::new(part_reader, compression)?);
        Ok(Self { reader: r })
    }

    pub fn determine_extension(&self) -> Option<&str> {
        self.reader.determine_extension()
    }
}

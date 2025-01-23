use std::io::{BufRead, Cursor, Read, Seek, SeekFrom};

use crate::error::Result;
use crate::pak::PakEntry;

use super::compressed::CompressedReader;
use super::encrypted::EncryptedReader;
use super::extension::ExtensionReader;

/// Read a pak entry.
pub struct PakEntryReader<R> {
    reader: ExtensionReader<CompressedReader<EncryptedReader<R>>>,
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
        let data_len = entry.compressed_size() as usize;

        reader.seek(SeekFrom::Start(entry.offset()))?;
        let mut data = vec![0; data_len];
        reader.read_exact(&mut data)?;
        let owned_reader = Cursor::new(data);

        let r = EncryptedReader::new(owned_reader, entry.encryption_type());
        let r = CompressedReader::new(r, entry.compression_type())?;
        let r = ExtensionReader::new(r);
        Ok(Self { reader: r })
    }
}

impl<R> PakEntryReader<R>
where
    R: BufRead,
{
    pub fn from_part_reader(part_reader: R, entry: &PakEntry) -> Result<Self> {
        let r = EncryptedReader::new(part_reader, entry.encryption_type());
        let r = CompressedReader::new(r, entry.compression_type())?;
        let r = ExtensionReader::new(r);
        Ok(Self { reader: r })
    }

    pub fn determine_extension(&self) -> Option<&str> {
        self.reader.determine_extension()
    }
}

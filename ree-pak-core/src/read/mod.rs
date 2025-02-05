use std::io::{Cursor, Read};

use crate::error::Result;
use crate::pak::{self, CompressionType, PakArchive, PakEntry, PakHeader};
use crate::spec;

pub mod archive;
pub mod compressed;
pub mod encrypted;
pub mod entry;
pub mod extension;

#[derive(Debug, thiserror::Error)]
pub enum PakReaderError {
    #[error("Failed to read raw data: {0}")]
    RawData(std::io::Error),
    #[error("Failed to decompress from {compression:?}: {source}")]
    Decompression {
        compression: CompressionType,
        source: std::io::Error,
    },
    #[error("Invalid compression type: {0}")]
    InvalidCompressionType(u8),
    #[error("Failed to determine file extension: {0}")]
    Extension(std::io::Error),
}

impl PakReaderError {
    pub fn into_io_error(self) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::Other, self.to_string())
    }
}

pub fn read_archive<R>(reader: &mut R) -> Result<PakArchive>
where
    R: Read,
{
    // read header
    let spec_header = spec::Header::from_reader(reader)?;
    let header = PakHeader::try_from(spec_header)?;

    // read entries
    let mut entry_table_bytes = vec![0; (header.entry_size() * header.total_files()) as usize];
    reader.read_exact(&mut entry_table_bytes)?;
    // decrypt
    if header.feature() == 8 {
        let mut raw_key = [0; 128];
        reader.read_exact(&mut raw_key)?;
        entry_table_bytes = pak::decrypt_pak_data(&entry_table_bytes, &raw_key);
    }
    // parse entries
    let entries = read_entries(&mut Cursor::new(&entry_table_bytes), &header)?;

    Ok(PakArchive::new(header, entries))
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

pub mod io;

use std::io::{Cursor, Read};

use crate::error::Result;
use crate::pak::{self, PakArchive, PakEntry, PakHeader};
use crate::spec;

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

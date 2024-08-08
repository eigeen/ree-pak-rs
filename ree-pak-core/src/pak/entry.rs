use std::io::Read;

use byteorder::{LittleEndian, ReadBytesExt};

type Result<T, E = EntryError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum EntryError {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
}

/// A table of entries in a PAK file.
#[derive(Debug, Clone)]
pub struct EntryTable {
    pub entries: Vec<Entry>,
}

impl EntryTable {
    pub fn from_iter<P, I, R>(iter: I) -> Result<Self>
    where
        P: EntryParser,
        I: IntoIterator<Item = R>,
        R: Read,
    {
        let mut entries = Vec::new();
        for reader in iter {
            let entry = Entry::from_reader::<P, _>(reader)?;
            entries.push(entry);
        }

        Ok(Self { entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

impl<'a> std::iter::IntoIterator for &'a EntryTable {
    type Item = &'a Entry;

    type IntoIter = std::slice::Iter<'a, Entry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

// impl<'data> rayon::iter::IntoParallelRefIterator<'data> for EntryTable {
//     type Iter = rayon::slice::Iter<'data, Entry>;

//     type Item = &'data Entry;

//     fn par_iter(&'data self) -> Self::Iter {
//         self.entries.par_iter()
//     }
// }

impl rayon::iter::IntoParallelIterator for EntryTable {
    type Item = Entry;

    type Iter = rayon::vec::IntoIter<Self::Item>;

    fn into_par_iter(self) -> Self::Iter {
        self.entries.into_par_iter()
    }
}

/// An file entry.
#[derive(Debug, Clone, Default)]
pub struct Entry {
    pub hash_name_lower: u32,
    pub hash_name_upper: u32,
    pub offset: i64,
    pub compressed_size: i64,
    pub uncompressed_size: i64,
    pub compression_method: CompressionMethod,
    pub checksum: u64,
}

impl Entry {
    pub fn from_reader<P, R>(reader: R) -> Result<Self>
    where
        P: EntryParser,
        R: Read,
    {
        P::from_reader(reader)
    }

    pub fn hash(&self) -> u64 {
        ((self.hash_name_upper as u64) << 32) | (self.hash_name_lower as u64)
    }
}

pub trait EntryParser {
    fn from_reader<R>(reader: R) -> Result<Entry>
    where
        R: Read;
}

pub struct EntryKindA;

#[allow(clippy::field_reassign_with_default)]
impl EntryParser for EntryKindA {
    fn from_reader<R>(mut reader: R) -> Result<Entry>
    where
        R: Read,
    {
        let mut entry = Entry::default();
        entry.offset = reader.read_i64::<LittleEndian>()?;
        entry.uncompressed_size = reader.read_i64::<LittleEndian>()?;
        entry.hash_name_lower = reader.read_u32::<LittleEndian>()?;
        entry.hash_name_upper = reader.read_u32::<LittleEndian>()?;

        Ok(entry)
    }
}

pub struct EntryKindB;

#[allow(clippy::field_reassign_with_default)]
impl EntryParser for EntryKindB {
    fn from_reader<R>(mut reader: R) -> Result<Entry>
    where
        R: Read,
    {
        let mut entry = Entry::default();
        entry.hash_name_lower = reader.read_u32::<LittleEndian>()?;
        entry.hash_name_upper = reader.read_u32::<LittleEndian>()?;
        entry.offset = reader.read_i64::<LittleEndian>()?;
        entry.compressed_size = reader.read_i64::<LittleEndian>()?;
        entry.uncompressed_size = reader.read_i64::<LittleEndian>()?;
        entry.compression_method = CompressionMethod::from_i64(reader.read_i64::<LittleEndian>()?);
        entry.checksum = reader.read_u64::<LittleEndian>()?;

        Ok(entry)
    }
}

#[derive(Debug, Clone, Default)]
#[repr(i64)]
pub enum CompressionMethod {
    #[default]
    None = 0,
    Deflate = 1,
    Zstd = 2,
}

impl CompressionMethod {
    pub fn from_i64(value: i64) -> Self {
        if value & 0xF == 1 {
            if value >> 16 > 0 {
                CompressionMethod::None
            } else {
                CompressionMethod::Deflate
            }
        } else if value & 0xF == 2 {
            if value >> 16 > 0 {
                CompressionMethod::None
            } else {
                CompressionMethod::Zstd
            }
        } else {
            Self::None
        }
    }
}

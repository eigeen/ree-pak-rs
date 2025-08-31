use std::io::Read;

use crate::error::Result;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct EntryV1 {
    pub offset: u64,
    pub uncompressed_size: u64,
    pub hash_name_lower: u32,
    pub hash_name_upper: u32,
}

static_assertions::assert_eq_size!(EntryV1, [u8; 24]);

impl EntryV1 {
    pub const SIZE: usize = std::mem::size_of::<Self>();

    pub fn from_reader<R>(reader: &mut R) -> Result<Self>
    where
        R: Read,
    {
        let mut buf = [0u8; Self::SIZE];
        reader.read_exact(&mut buf)?;
        Ok(Self::read_from_bytes(&buf).unwrap())
    }

    pub fn into_bytes(self) -> [u8; Self::SIZE] {
        self.as_bytes().try_into().unwrap()
    }
}

#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct EntryV2 {
    pub hash_name_lower: u32,
    pub hash_name_upper: u32,
    pub offset: u64,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub attributes: i64,
    pub checksum: u64,
}

static_assertions::assert_eq_size!(EntryV2, [u8; 48]);

impl EntryV2 {
    pub const SIZE: usize = std::mem::size_of::<Self>();

    pub fn from_reader<R>(reader: &mut R) -> Result<Self>
    where
        R: Read,
    {
        let mut buf = [0u8; Self::SIZE];
        reader.read_exact(&mut buf)?;
        Ok(Self::read_from_bytes(&buf).unwrap())
    }

    pub fn into_bytes(self) -> [u8; Self::SIZE] {
        self.as_bytes().try_into().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_write() {
        let bytes = &[
            0x34, 0x2F, 0x6E, 0xC2, 0xEB, 0xBE, 0xE6, 0x80, 0x95, 0xFA, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x8A,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x8A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let entry = EntryV2::read_from_bytes(bytes).unwrap();
        assert_eq!(entry.hash_name_lower, 3262000948);
        assert_eq!(entry.hash_name_upper, 2162605803);
        assert_eq!(entry.offset, 457365);
        assert_eq!(entry.compressed_size, 35392);
        assert_eq!(entry.uncompressed_size, 35392);
        assert_eq!(entry.attributes, 0);
        assert_eq!(entry.checksum, 0);

        let write_bytes = entry.into_bytes();
        assert_eq!(write_bytes, *bytes);
    }
}

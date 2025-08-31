use std::io::Read;

use crate::error::Result;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct Header {
    pub magic: [u8; 4],
    pub major_version: u8,
    pub minor_version: u8,
    pub feature: u16,
    pub total_files: u32,
    pub hash: u32,
}

static_assertions::assert_eq_size!(Header, [u8; 16]);

impl Header {
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

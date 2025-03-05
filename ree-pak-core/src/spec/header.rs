use std::io::Read;

use crate::error::Result;

#[derive(Debug, Clone)]
#[repr(C)]
pub struct Header {
    pub magic: [u8; 4],
    pub major_version: u8,
    pub minor_version: u8,
    pub feature: u16,
    pub total_files: u32,
    pub hash: u32,
}

impl Header {
    pub const SIZE: usize = std::mem::size_of::<Self>();

    pub fn from_reader<R>(reader: &mut R) -> Result<Self>
    where
        R: Read,
    {
        let mut buf = [0u8; Self::SIZE];
        reader.read_exact(&mut buf)?;
        unsafe { Ok(std::mem::transmute::<[u8; Self::SIZE], Self>(buf)) }
    }

    pub fn into_bytes(self) -> [u8; Self::SIZE] {
        unsafe { std::mem::transmute::<Self, [u8; Self::SIZE]>(self) }
    }
}

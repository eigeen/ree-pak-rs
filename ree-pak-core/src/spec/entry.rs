use std::io::Read;

use crate::error::Result;

#[derive(Debug, Clone)]
#[repr(C)]
pub struct EntryV1 {
    pub offset: u64,
    pub uncompressed_size: u64,
    pub hash_name_lower: u32,
    pub hash_name_upper: u32,
}

impl EntryV1 {
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
        unsafe {
            let bytes: [u8; Self::SIZE] = std::mem::transmute(self);
            bytes
        }
    }
}

#[derive(Debug, Clone)]
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

impl EntryV2 {
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
        unsafe {
            let bytes: [u8; Self::SIZE] = std::mem::transmute(self);
            bytes
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_size() {
        assert_eq!(std::mem::size_of::<EntryV1>(), 24);
        assert_eq!(std::mem::size_of::<EntryV2>(), 48);
    }
}

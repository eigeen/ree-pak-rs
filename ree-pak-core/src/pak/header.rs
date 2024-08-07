use std::io::Read;

use byteorder::{LittleEndian, ReadBytesExt};

type Result<T, E = HeaderError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum HeaderError {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Invalid magic: expected {expected:?}, got {got:?}")]
    InvalidMagic {
        expected: &'static [u8; 4],
        got: [u8; 4],
    },
    #[error("Unsupported version: {major}.{minor}")]
    UnsupportedVersion { major: i8, minor: i8 },
    #[error("Unsupported algorithm: got code={alg_code}")]
    UnsupportedAlgorithm { alg_code: i16 },
}

#[derive(Debug, Clone, Default)]
pub struct PackageHeader {
    pub magic: [u8; 4],
    pub major_version: i8,
    pub minor_version: i8,
    pub feature: i16,
    pub total_files: u32,
    pub hash: i32,
}

impl PackageHeader {
    pub fn from_reader<R>(mut reader: R) -> Result<PackageHeader>
    where
        R: Read,
    {
        let mut this = PackageHeader::default();

        let mut magic = [0; 4];
        reader.read_exact(&mut magic)?;
        this.magic = magic;
        this.major_version = reader.read_i8()?;
        this.minor_version = reader.read_i8()?;
        this.feature = reader.read_i16::<LittleEndian>()?;
        this.total_files = reader.read_u32::<LittleEndian>()?;
        this.hash = reader.read_i32::<LittleEndian>()?;

        if &this.magic != b"KPKA" {
            return Err(HeaderError::InvalidMagic {
                expected: b"KPKA",
                got: magic,
            });
        }
        if ![2, 4].contains(&this.major_version) || ![0, 1].contains(&this.minor_version) {
            return Err(HeaderError::UnsupportedVersion {
                major: this.major_version,
                minor: this.minor_version,
            });
        }
        if ![0, 8].contains(&this.feature) {
            return Err(HeaderError::UnsupportedAlgorithm {
                alg_code: this.feature,
            });
        }

        Ok(this)
    }

    pub fn entry_size(&self) -> u32 {
        match self.major_version {
            2 => 24,
            4 => 48,
            _ => panic!("Unsupported major version"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_header() {
        let bytes = [
            0x4B, 0x50, 0x4B, 0x41, 0x04, 0x00, 0x08, 0x00, 0x2D, 0x9C, 0x00, 0x00, 0x95, 0x41,
            0x39, 0x9F,
        ];
        let _header = PackageHeader::from_reader(&bytes[..]).unwrap();
    }
}

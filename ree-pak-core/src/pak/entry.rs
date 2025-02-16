use serde::Serialize;

use crate::serde_util::{serialize_u32_hex, serialize_u64_hex};
use crate::spec;

use super::flag::{CompressionType, EncryptionType};

#[derive(Clone, Default, Serialize)]
pub struct PakEntry {
    #[serde(serialize_with = "serialize_u32_hex")]
    hash_name_lower: u32,
    #[serde(serialize_with = "serialize_u32_hex")]
    hash_name_upper: u32,
    offset: u64,
    compressed_size: u64,
    uncompressed_size: u64,
    compression_type: CompressionType,
    encryption_type: EncryptionType,
    #[serde(serialize_with = "serialize_u64_hex")]
    checksum: u64,
}

impl PakEntry {
    pub fn hash(&self) -> u64 {
        let upper = self.hash_name_upper as u64;
        let lower = self.hash_name_lower as u64;

        upper << 32 | lower
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn compressed_size(&self) -> u64 {
        self.compressed_size
    }

    pub fn uncompressed_size(&self) -> u64 {
        self.uncompressed_size
    }

    pub fn compression_type(&self) -> CompressionType {
        self.compression_type
    }

    pub fn encryption_type(&self) -> EncryptionType {
        self.encryption_type
    }

    pub fn checksum(&self) -> u64 {
        self.checksum
    }
}

impl From<spec::EntryV1> for PakEntry {
    fn from(value: spec::EntryV1) -> Self {
        Self {
            hash_name_lower: value.hash_name_lower,
            hash_name_upper: value.hash_name_upper,
            offset: value.offset,
            uncompressed_size: value.uncompressed_size,
            ..Default::default()
        }
    }
}

impl From<spec::EntryV2> for PakEntry {
    fn from(value: spec::EntryV2) -> Self {
        Self {
            hash_name_lower: value.hash_name_lower,
            hash_name_upper: value.hash_name_upper,
            offset: value.offset,
            compressed_size: value.compressed_size,
            uncompressed_size: value.uncompressed_size,
            compression_type: CompressionType::from_bits_truncate((value.attributes & 0xF) as u8),
            encryption_type: (((value.attributes & 0x00FF0000) >> 16) as u32).into(),
            checksum: value.checksum,
        }
    }
}

impl std::fmt::Debug for PakEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PakEntry")
            .field("hash_name_lower", &format!("{:08x}", self.hash_name_lower))
            .field("hash_name_upper", &format!("{:08x}", self.hash_name_upper))
            .field("offset", &self.offset)
            .field("compressed_size", &self.compressed_size)
            .field("uncompressed_size", &self.uncompressed_size)
            .field("compression_type", &self.compression_type)
            .field("encryption_type", &self.encryption_type)
            .field("checksum", &format!("{:16x}", self.checksum))
            .finish()
    }
}

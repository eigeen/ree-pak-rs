use serde::{Deserialize, Serialize};

use crate::serde_util::{serde_u32_hex, serde_u64_hex};
use crate::spec;

use super::flag::{CompressionType, EncryptionType, UnkAttr};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct PakEntry {
    #[serde(with = "serde_u32_hex")]
    pub(crate) hash_name_lower: u32,
    #[serde(with = "serde_u32_hex")]
    pub(crate) hash_name_upper: u32,
    pub(crate) offset: u64,
    pub(crate) compressed_size: u64,
    pub(crate) uncompressed_size: u64,
    pub(crate) compression_type: CompressionType,
    pub(crate) encryption_type: EncryptionType,
    #[serde(with = "serde_u64_hex")]
    pub(crate) checksum: u64,
    pub(crate) unk_attr: UnkAttr,
}

impl PakEntry {
    pub fn hash(&self) -> u64 {
        let upper = self.hash_name_upper as u64;
        let lower = self.hash_name_lower as u64;

        (upper << 32) | lower
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

    pub fn unk_attr(&self) -> &UnkAttr {
        &self.unk_attr
    }

    pub fn into_bytes_v2(self) -> Vec<u8> {
        let entry_v2 = spec::EntryV2::from(self.clone());
        entry_v2.into_bytes().to_vec()
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
            unk_attr: UnkAttr::from_bits_truncate(value.attributes as u64),
        }
    }
}

impl From<PakEntry> for spec::EntryV2 {
    fn from(value: PakEntry) -> Self {
        let attr_known = (value.compression_type.bits() as i64) | (((value.encryption_type as u32) as i64) << 16);
        let attr = attr_known | value.unk_attr.bits() as i64;

        Self {
            hash_name_lower: value.hash_name_lower,
            hash_name_upper: value.hash_name_upper,
            offset: value.offset,
            compressed_size: value.compressed_size,
            uncompressed_size: value.uncompressed_size,
            attributes: attr,
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
            .field("checksum", &format!("{:016x}", self.checksum))
            .finish()
    }
}

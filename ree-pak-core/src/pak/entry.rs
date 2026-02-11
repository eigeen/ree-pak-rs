use serde::{Deserialize, Serialize};

use crate::serde_util::{serde_u32_hex, serde_u64_hex};
use crate::spec;

use super::flag::{CompressionType, EncryptionType, KnownAttr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryOffset {
    FileOffset(u64),
    ChunkIndex(u64),
}

impl Default for EntryOffset {
    fn default() -> Self {
        EntryOffset::FileOffset(0)
    }
}

impl EntryOffset {
    pub fn raw(self) -> u64 {
        match self {
            EntryOffset::FileOffset(v) => v,
            EntryOffset::ChunkIndex(v) => v,
        }
    }

    pub fn file_offset(self) -> Option<u64> {
        match self {
            EntryOffset::FileOffset(v) => Some(v),
            EntryOffset::ChunkIndex(_) => None,
        }
    }

    pub fn chunk_index(self) -> Option<u64> {
        match self {
            EntryOffset::FileOffset(_) => None,
            EntryOffset::ChunkIndex(v) => Some(v),
        }
    }

    pub fn is_chunk_index(self) -> bool {
        matches!(self, EntryOffset::ChunkIndex(_))
    }
}

// Keep JSON output stable: `offset` stays a plain number.
impl Serialize for EntryOffset {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u64(self.raw())
    }
}

impl<'de> Deserialize<'de> for EntryOffset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        Ok(EntryOffset::FileOffset(value))
    }
}

#[derive(Clone, Default, Serialize, Deserialize, derive_more::Debug)]
pub struct PakEntry {
    #[serde(with = "serde_u32_hex")]
    #[debug("0x{hash_name_lower:08x}")]
    pub(crate) hash_name_lower: u32,

    #[serde(with = "serde_u32_hex")]
    #[debug("0x{hash_name_upper:08x}")]
    pub(crate) hash_name_upper: u32,

    pub(crate) offset: EntryOffset,
    pub(crate) compressed_size: u64,
    pub(crate) uncompressed_size: u64,
    pub(crate) compression_type: CompressionType,
    pub(crate) encryption_type: EncryptionType,

    #[serde(with = "serde_u64_hex")]
    #[debug("0x{checksum:16x}")]
    pub(crate) checksum: u64,

    #[serde(skip)]
    pub(crate) known_attr: KnownAttr,

    /// Raw `attributes` field from `spec::EntryV2` (full u64 bit-pattern).
    pub(crate) all_attr: u64,
}

impl PakEntry {
    pub fn hash(&self) -> u64 {
        let upper = self.hash_name_upper as u64;
        let lower = self.hash_name_lower as u64;

        (upper << 32) | lower
    }

    pub fn offset(&self) -> EntryOffset {
        self.offset
    }

    pub fn offset_raw(&self) -> u64 {
        self.offset.raw()
    }

    pub fn file_offset(&self) -> Option<u64> {
        self.offset.file_offset()
    }

    pub fn chunk_index(&self) -> Option<u64> {
        self.offset.chunk_index()
    }

    pub fn offset_is_chunk_index(&self) -> bool {
        self.offset.is_chunk_index()
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

    pub fn known_attr(&self) -> KnownAttr {
        self.known_attr
    }

    pub fn all_attr(&self) -> u64 {
        self.all_attr
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
            offset: EntryOffset::FileOffset(value.offset),
            uncompressed_size: value.uncompressed_size,
            ..Default::default()
        }
    }
}

impl From<spec::EntryV2> for PakEntry {
    fn from(value: spec::EntryV2) -> Self {
        let all_attr = value.attributes as u64;
        let known_attr = KnownAttr::from_all_attr(all_attr);
        let offset = if known_attr.offset_is_chunk_index() {
            EntryOffset::ChunkIndex(value.offset)
        } else {
            EntryOffset::FileOffset(value.offset)
        };
        Self {
            hash_name_lower: value.hash_name_lower,
            hash_name_upper: value.hash_name_upper,
            offset,
            compressed_size: value.compressed_size,
            uncompressed_size: value.uncompressed_size,
            compression_type: CompressionType::from_bits_truncate(known_attr.compression_bits()),
            encryption_type: known_attr.encryption_bits().into(),
            checksum: value.checksum,
            known_attr,
            all_attr,
        }
    }
}

impl From<PakEntry> for spec::EntryV2 {
    fn from(value: PakEntry) -> Self {
        const ATTR_COMPRESSION_MASK: u64 = 0x0000_0000_0000_000F;
        const ATTR_ENCRYPTION_MASK: u64 = 0x0000_0000_00FF_0000;
        const ATTR_KNOWN_MASK: u64 = KnownAttr::KNOWN_MASK;

        let mut all_attr = value.all_attr;
        // Ensure fields are the source of truth for these known sub-fields.
        // Unknown bits are preserved as-is to allow round-tripping flags not yet understood.
        all_attr &= !(ATTR_COMPRESSION_MASK | ATTR_ENCRYPTION_MASK | ATTR_KNOWN_MASK);
        all_attr |= value.compression_type.bits() as u64;
        all_attr |= (value.encryption_type as u32 as u64) << 16;
        all_attr |= value.known_attr.bits();

        Self {
            hash_name_lower: value.hash_name_lower,
            hash_name_upper: value.hash_name_upper,
            offset: value.offset.raw(),
            compressed_size: value.compressed_size,
            uncompressed_size: value.uncompressed_size,
            attributes: all_attr as i64,
            checksum: value.checksum,
        }
    }
}

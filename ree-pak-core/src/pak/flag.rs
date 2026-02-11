use bitflags::bitflags;
use serde::{Deserialize, Serialize};

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Default)]
    pub struct CompressionType: u8 {
        const NONE = 0;
        const DEFLATE = 1;
        const ZSTD = 2;
    }
}

impl Serialize for CompressionType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(self.bits())
    }
}

impl<'de> Deserialize<'de> for CompressionType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        Ok(CompressionType::from_bits_truncate(value))
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncryptionType {
    #[default]
    None = 0,
    Type1 = 0x1, // pkc_key::c1n & pkc_key::c1d
    Type2 = 0x2, // pkc_key::c2n & pkc_key::c2d
    Type3 = 0x3, // pkc_key::c3n & pkc_key::c3d
    Type4 = 0x4, // pkc_key::c4n & pkc_key::c4d
    TypeInvalid = 0x5,
}

impl From<u32> for EncryptionType {
    fn from(value: u32) -> Self {
        match value {
            0 => EncryptionType::None,
            1 => EncryptionType::Type1,
            2 => EncryptionType::Type2,
            3 => EncryptionType::Type3,
            4 => EncryptionType::Type4,
            _ => EncryptionType::TypeInvalid,
        }
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct FeatureFlags: u16 {
        const BIT00 = 1 << 0;
        const BIT01 = 1 << 1;
        const BIT02 = 1 << 2;
        const ENTRY_ENCRYPTION = 1 << 3;
        /// Pak contains an extra u32 data after the TOC.
        const EXTRA_U32 = 1 << 4;
        /// Pak contains an extra chunk table after the TOC.
        ///
        /// When this flag is set, some entries may store `offset` as a chunk index
        /// (see `PakEntry::offset_is_chunk_index()`).
        const CHUNK_TABLE = 1 << 5;
        const BIT06 = 1 << 6;
        const BIT07 = 1 << 7;
        const BIT08 = 1 << 8;
        const BIT09 = 1 << 9;
        const BIT10 = 1 << 10;
        const BIT11 = 1 << 11;
        const BIT12 = 1 << 12;
        const BIT13 = 1 << 13;
        const BIT14 = 1 << 14;
        const BIT15 = 1 << 15;
    }
}

impl FeatureFlags {
    pub fn check_supported(&self) -> bool {
        let supported_flags = FeatureFlags::ENTRY_ENCRYPTION | FeatureFlags::EXTRA_U32 | FeatureFlags::CHUNK_TABLE;
        self.bits() & !supported_flags.bits() == 0
    }
}

impl Serialize for FeatureFlags {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u16(self.bits())
    }
}

impl<'de> Deserialize<'de> for FeatureFlags {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u16::deserialize(deserializer)?;
        Ok(FeatureFlags::from_bits_truncate(value))
    }
}

/// Known sub-fields within the raw entry `attributes` (EntryV2).
///
/// This is intentionally *not* a `bitflags!` type because the `attributes` field includes
/// both single-bit flags and multi-bit fields (e.g. compression/encryption).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KnownAttr {
    bits: u64,
}

impl KnownAttr {
    pub const COMPRESSION_MASK: u64 = 0x0000_0000_0000_000F;
    pub const ENCRYPTION_MASK: u64 = 0x0000_0000_00FF_0000;
    pub const OFFSET_IS_CHUNK_INDEX: u64 = 1 << 24;

    pub const KNOWN_MASK: u64 = Self::COMPRESSION_MASK | Self::ENCRYPTION_MASK | Self::OFFSET_IS_CHUNK_INDEX;

    pub fn from_all_attr(all_attr: u64) -> Self {
        Self {
            bits: all_attr & Self::KNOWN_MASK,
        }
    }

    pub fn bits(self) -> u64 {
        self.bits
    }

    pub fn compression_bits(self) -> u8 {
        (self.bits & Self::COMPRESSION_MASK) as u8
    }

    pub fn encryption_bits(self) -> u32 {
        ((self.bits & Self::ENCRYPTION_MASK) >> 16) as u32
    }

    pub fn offset_is_chunk_index(self) -> bool {
        (self.bits & Self::OFFSET_IS_CHUNK_INDEX) != 0
    }
}

impl Serialize for KnownAttr {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u64(self.bits)
    }
}

impl<'de> Deserialize<'de> for KnownAttr {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        Ok(KnownAttr {
            bits: value & KnownAttr::KNOWN_MASK,
        })
    }
}

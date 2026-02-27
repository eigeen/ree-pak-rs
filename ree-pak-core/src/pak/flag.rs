use bitflags::bitflags;
use serde::{Deserialize, Serialize};

#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CompressionType {
    #[default]
    None = 0,
    Deflate = 1,
    Zstd = 2,
}

impl CompressionType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Deflate),
            2 => Some(Self::Zstd),
            _ => None,
        }
    }

    pub fn bits(self) -> u8 {
        self as u8
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
        CompressionType::from_u8(value)
            .ok_or_else(|| serde::de::Error::custom(format!("Invalid compression type: {value}")))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChunkCompressionType {
    /// Stored (uncompressed) chunk.
    #[default]
    None,
    /// Zstd-compressed chunk.
    Zstd,
}

impl ChunkCompressionType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Zstd),
            _ => None,
        }
    }

    pub fn bits(self) -> u8 {
        self as u8
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
    /// Feature flags stored in the pak header.
    pub struct FeatureFlags: u16 {
        const BIT00 = 1 << 0;
        const BIT01 = 1 << 1;
        /// Pak contains an extra 9 bytes of data after the TOC (entry table) and before the 128-byte entry encryption key.
        ///
        /// First appears in RE9
        const EXTRA_DATA = 1 << 2;
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
    const SUPPORTED_BITS: u16 =
        Self::EXTRA_DATA.bits() | Self::ENTRY_ENCRYPTION.bits() | Self::EXTRA_U32.bits() | Self::CHUNK_TABLE.bits();

    /// Back-compat alias for older versions of this crate (bit 0x4).
    pub const BIT02: FeatureFlags = FeatureFlags::EXTRA_DATA;

    /// Returns `true` if all set flags are supported by this crate.
    pub fn check_supported(&self) -> bool {
        self.unsupported_bits() == 0
    }

    /// Return the raw bits that are not currently supported by this crate.
    pub fn unsupported_bits(&self) -> u16 {
        self.bits() & !Self::SUPPORTED_BITS
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

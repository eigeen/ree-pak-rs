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
        const EXTRA_U32 = 1 << 4;
        const UNK_1 = 1 << 5;
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
        let supported_flags = FeatureFlags::ENTRY_ENCRYPTION | FeatureFlags::EXTRA_U32 | FeatureFlags::UNK_1;
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

bitflags! {
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct UnkAttr: u64 {
        // const BIT00 = 1 << 0;
        // const BIT01 = 1 << 1;
        const BIT02 = 1 << 2;
        const BIT03 = 1 << 3;
        const BIT04 = 1 << 4;
        const BIT05 = 1 << 5;
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
        // const BIT16 = 1 << 16;
        // const BIT17 = 1 << 17;
        const BIT18 = 1 << 18;
        const BIT19 = 1 << 19;
        const BIT20 = 1 << 20;
        const BIT21 = 1 << 21;
        const BIT22 = 1 << 22;
        const BIT23 = 1 << 23;
        const BIT24 = 1 << 24;
        const BIT25 = 1 << 25;
        const BIT26 = 1 << 26;
        const BIT27 = 1 << 27;
        const BIT28 = 1 << 28;
        const BIT29 = 1 << 29;
        const BIT30 = 1 << 30;
        const BIT31 = 1 << 31;
        const BIT32 = 1 << 32;
        const BIT33 = 1 << 33;
        const BIT34 = 1 << 34;
        const BIT35 = 1 << 35;
        const BIT36 = 1 << 36;
        const BIT37 = 1 << 37;
        const BIT38 = 1 << 38;
        const BIT39 = 1 << 39;
        const BIT40 = 1 << 40;
        const BIT41 = 1 << 41;
        const BIT42 = 1 << 42;
        const BIT43 = 1 << 43;
        const BIT44 = 1 << 44;
        const BIT45 = 1 << 45;
        const BIT46 = 1 << 46;
        const BIT47 = 1 << 47;
        const BIT48 = 1 << 48;
        const BIT49 = 1 << 49;
        const BIT50 = 1 << 50;
        const BIT51 = 1 << 51;
        const BIT52 = 1 << 52;
        const BIT53 = 1 << 53;
        const BIT54 = 1 << 54;
        const BIT55 = 1 << 55;
        const BIT56 = 1 << 56;
        const BIT57 = 1 << 57;
        const BIT58 = 1 << 58;
        const BIT59 = 1 << 59;
        const BIT60 = 1 << 60;
        const BIT61 = 1 << 61;
        const BIT62 = 1 << 62;
        const BIT63 = 1 << 63;
    }
}

impl Serialize for UnkAttr {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u64(self.bits())
    }
}

impl<'de> Deserialize<'de> for UnkAttr {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        Ok(UnkAttr::from_bits_truncate(value))
    }
}

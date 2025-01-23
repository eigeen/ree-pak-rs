use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, Default)]
    pub struct CompressionType: u8 {
        const NONE = 0;
        const DEFLATE = 1;
        const ZSTD = 2;
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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

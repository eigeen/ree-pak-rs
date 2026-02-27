use serde::{Deserialize, Serialize};

use crate::serde_util::serde_u32_hex;
use crate::spec;

use super::FeatureFlags;

const HEADER_MAGIC: &[u8; 4] = b"KPKA";

/// Pak file header (TOC header).
#[derive(Clone, Serialize, Deserialize, derive_more::Debug)]
pub struct PakHeader {
    #[debug("{magic:02x?}")]
    pub(crate) magic: [u8; 4],
    pub(crate) major_version: u8,
    pub(crate) minor_version: u8,
    pub(crate) feature: FeatureFlags,
    pub(crate) total_files: u32,
    // didn't really understand this field, probably signature or fingerprint.
    #[serde(with = "serde_u32_hex")]
    #[debug("{hash:08x}")]
    pub(crate) hash: u32,
    /// another unknown field
    /// if feature contains specific flag,
    /// the value will between the resource headers
    /// and the 128 byte signature bytes.
    pub(crate) unk_u32_sig: u32,
}

impl Default for PakHeader {
    fn default() -> Self {
        Self {
            magic: *HEADER_MAGIC,
            major_version: 0,
            minor_version: 0,
            feature: FeatureFlags::default(),
            total_files: 0,
            hash: 0,
            unk_u32_sig: 0,
        }
    }
}

impl PakHeader {
    pub fn entry_size(&self) -> u32 {
        match self.major_version {
            2 => spec::EntryV1::SIZE as u32,
            4 => spec::EntryV2::SIZE as u32,
            _ => panic!("Unsupported major version. Unreachable code."),
        }
    }

    pub fn magic(&self) -> [u8; 4] {
        self.magic
    }

    pub fn major_version(&self) -> u8 {
        self.major_version
    }

    pub fn minor_version(&self) -> u8 {
        self.minor_version
    }

    pub fn feature(&self) -> FeatureFlags {
        self.feature
    }

    pub fn total_files(&self) -> u32 {
        self.total_files
    }

    pub fn hash(&self) -> u32 {
        self.hash
    }

    pub fn into_bytes(self) -> Vec<u8> {
        let spec_header = spec::Header::from(self);
        spec_header.into_bytes().to_vec()
    }
}

impl TryFrom<spec::Header> for PakHeader {
    type Error = crate::error::PakError;

    fn try_from(this: spec::Header) -> Result<Self, Self::Error> {
        Self::try_from_spec_with_strict_feature_flags(this, true)
    }
}

impl From<PakHeader> for spec::Header {
    fn from(value: PakHeader) -> Self {
        spec::Header {
            magic: value.magic,
            major_version: value.major_version,
            minor_version: value.minor_version,
            feature: value.feature.bits(),
            total_files: value.total_files,
            hash: value.hash,
        }
    }
}

impl PakHeader {
    pub(crate) fn try_from_spec_with_strict_feature_flags(
        this: spec::Header,
        strict_feature_flags: bool,
    ) -> Result<Self, crate::error::PakError> {
        if &this.magic != HEADER_MAGIC {
            return Err(crate::error::PakError::InvalidMagic {
                expected: *HEADER_MAGIC,
                found: this.magic,
            });
        }
        if ![2, 4].contains(&this.major_version) || ![0, 1, 2].contains(&this.minor_version) {
            return Err(crate::error::PakError::UnsupportedVersion {
                major: this.major_version,
                minor: this.minor_version,
            });
        }
        let feature = FeatureFlags::from_bits_truncate(this.feature);
        if strict_feature_flags && !feature.check_supported() {
            return Err(crate::error::PakError::UnsupportedFeature(feature));
        }

        Ok(PakHeader {
            magic: this.magic,
            major_version: this.major_version,
            minor_version: this.minor_version,
            feature,
            total_files: this.total_files,
            hash: this.hash,
            unk_u32_sig: 0, // TODO
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_strict_feature_flags_errors() {
        let h = spec::Header {
            magic: *HEADER_MAGIC,
            major_version: 4,
            minor_version: 2,
            feature: (FeatureFlags::ENTRY_ENCRYPTION | FeatureFlags::BIT02).bits(),
            total_files: 0,
            hash: 0,
        };

        let err = PakHeader::try_from_spec_with_strict_feature_flags(h, true).unwrap_err();
        assert!(matches!(err, crate::error::PakError::UnsupportedFeature(_)));
    }

    #[test]
    fn header_lenient_feature_flags_allows_unsupported() {
        let h = spec::Header {
            magic: *HEADER_MAGIC,
            major_version: 4,
            minor_version: 2,
            feature: (FeatureFlags::ENTRY_ENCRYPTION | FeatureFlags::BIT02).bits(),
            total_files: 0,
            hash: 0,
        };

        let header = PakHeader::try_from_spec_with_strict_feature_flags(h, false).unwrap();
        assert_eq!(header.feature().unsupported_bits(), FeatureFlags::BIT02.bits());
    }
}

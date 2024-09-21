use crate::spec;

#[derive(Clone, Default)]
pub struct PakHeader {
    magic: [u8; 4],
    major_version: u8,
    minor_version: u8,
    feature: u16,
    total_files: u32,
    hash: u32,
}

impl PakHeader {
    pub fn entry_size(&self) -> u32 {
        match self.major_version {
            2 => 24,
            4 => 48,
            _ => panic!("Unsupported major version"),
        }
    }

    #[inline]
    pub fn magic(&self) -> [u8; 4] {
        self.magic
    }

    #[inline]
    pub fn major_version(&self) -> u8 {
        self.major_version
    }

    #[inline]
    pub fn minor_version(&self) -> u8 {
        self.minor_version
    }

    #[inline]
    pub fn feature(&self) -> u16 {
        self.feature
    }

    #[inline]
    pub fn total_files(&self) -> u32 {
        self.total_files
    }

    #[inline]
    pub fn hash(&self) -> u32 {
        self.hash
    }
}

impl TryFrom<spec::Header> for PakHeader {
    type Error = crate::error::PakError;

    fn try_from(this: spec::Header) -> Result<Self, Self::Error> {
        if &this.magic != b"KPKA" {
            return Err(Self::Error::InvalidMagic {
                expected: *b"KPKA",
                found: this.magic,
            });
        }
        if (this.major_version != 2 && this.major_version != 4) || ![0, 1].contains(&this.minor_version) {
            return Err(Self::Error::UnsupportedVersion {
                major: this.major_version,
                minor: this.minor_version,
            });
        }
        if ![0, 8].contains(&this.feature) {
            return Err(Self::Error::UnsupportedAlgorithm(this.feature));
        }

        Ok(PakHeader {
            magic: this.magic,
            major_version: this.major_version,
            minor_version: this.minor_version,
            feature: this.feature,
            total_files: this.total_files,
            hash: this.hash,
        })
    }
}

impl std::fmt::Debug for PakHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PakHeader")
            .field("magic", &format!("{:02x?}", self.magic))
            .field("major_version", &self.major_version)
            .field("minor_version", &self.minor_version)
            .field("feature", &self.feature)
            .field("total_files", &self.total_files)
            .field("hash", &format!("{:08x}", self.hash))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_size() {
        assert_eq!(std::mem::size_of::<PakHeader>(), 16);
    }
}

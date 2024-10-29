use std::{collections::HashMap, hash::BuildHasherDefault, path::Path, sync::Mutex};

use nohash::NoHashHasher;
use rayon::iter::{ParallelBridge, ParallelIterator};

use crate::error::Result;

#[derive(Debug, Clone, Default)]
pub struct FileNameTable {
    file_names: HashMap<u64, FileName, BuildHasherDefault<NoHashHasher<u64>>>,
}

impl FileNameTable {
    pub fn from_list_file<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let file_names = std::fs::read_to_string(path.as_ref())?;
        let this = Mutex::new(Self::default());
        file_names.lines().par_bridge().for_each(|line| {
            let file_name = FileName::new(line);
            let hash = file_name.hash_mixed();
            this.lock().unwrap().file_names.insert(hash, file_name);
        });

        Ok(this.into_inner().unwrap())
    }

    pub fn push_str(&mut self, file_name: &str) {
        let file_name = FileName::new(file_name);
        let hash = file_name.hash_mixed();
        self.file_names.insert(hash, file_name);
    }

    pub fn get_file_name(&self, hash: u64) -> Option<&FileName> {
        self.file_names.get(&hash)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileName {
    name: String,
}

impl FileName {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string() }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn hash_lower_case(&self) -> u32 {
        let bytes: Vec<u8> = self
            .name
            .to_lowercase()
            .encode_utf16()
            .flat_map(|c| c.to_le_bytes())
            .collect();

        murmur3_hash(&bytes[..]).unwrap()
    }

    pub fn hash_upper_case(&self) -> u32 {
        let bytes: Vec<u8> = self
            .name
            .to_uppercase()
            .encode_utf16()
            .flat_map(|c| c.to_le_bytes())
            .collect();

        murmur3_hash(&bytes[..]).unwrap()
    }

    pub fn hash_mixed(&self) -> u64 {
        Self::mix_hash(self.hash_lower_case(), self.hash_upper_case())
    }

    pub fn mix_hash(lower: u32, upper: u32) -> u64 {
        let upper = upper as u64;
        let lower = lower as u64;

        upper << 32 | lower
    }
}

pub fn murmur3_hash<R: std::io::Read>(mut reader: R) -> Result<u32> {
    Ok(murmur3::murmur3_32(&mut reader, 0xFFFFFFFF)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_file_name() {
        let filename = FileName::new("natives/stm/camera/collisionfilter/defaultcamera.cfil.7");
        assert_eq!(filename.hash_lower_case(), 0x65B486A1);
        assert_eq!(filename.hash_upper_case(), 0x958EDD0C);
        assert_eq!(filename.hash_mixed(), 0x958EDD0C65B486A1);
    }
}

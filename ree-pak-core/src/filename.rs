use std::{collections::HashMap, io::Read, path::Path};

use nohash::BuildNoHashHasher;
use parking_lot::Mutex;
use rayon::iter::{ParallelBridge, ParallelIterator};

use crate::error::{PakError, Result};

#[derive(Debug, Clone, Default)]
pub struct FileNameTable {
    file_names: HashMap<u64, FileName, BuildNoHashHasher<u64>>,
}

impl FileNameTable {
    pub fn file_names(&self) -> impl Iterator<Item = (&u64, &FileName)> {
        self.file_names.iter()
    }

    pub fn from_list_file<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let content = std::fs::read(path.as_ref())?;
        let file_names = Self::parse_raw_file_names(&content)?;
        Self::from_list(file_names)
    }

    pub fn from_list(file_names: String) -> Result<Self> {
        let this = Mutex::new(Self::default());
        file_names.lines().par_bridge().for_each(|line| {
            let file_name = FileName::new(&line.replace('\\', "/"));
            let hash = file_name.hash_mixed();
            this.lock().file_names.insert(hash, file_name);
        });

        Ok(this.into_inner())
    }

    pub fn push_str(&mut self, file_name: &str) {
        let file_name = FileName::new(&file_name.replace('\\', "/"));
        let hash = file_name.hash_mixed();
        self.file_names.insert(hash, file_name);
    }

    pub fn get_file_name(&self, hash: u64) -> Option<&FileName> {
        self.file_names.get(&hash)
    }

    fn parse_raw_file_names(bytes: &[u8]) -> Result<String> {
        // is zstd
        if bytes[0..4] == [0x28, 0xB5, 0x2F, 0xFD] {
            let mut decoder = zstd::Decoder::new(bytes)?;
            let mut output = Vec::new();
            decoder.read_to_end(&mut output)?;
            String::from_utf8(output)
        } else {
            // plain text
            String::from_utf8(bytes.to_vec())
        }
        .map_err(|e| PakError::InvalidFileList(Box::new(e)))
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

        (upper << 32) | lower
    }
}

pub fn murmur3_hash<R: std::io::Read>(mut reader: R) -> Result<u32> {
    Ok(murmur3::murmur3_32(&mut reader, 0xFFFFFFFF)?)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn test_hash_file_name() {
        let filename = FileName::new("natives/stm/camera/collisionfilter/defaultcamera.cfil.7");
        assert_eq!(filename.hash_lower_case(), 0x65B486A1);
        assert_eq!(filename.hash_upper_case(), 0x958EDD0C);
        assert_eq!(filename.hash_mixed(), 0x958EDD0C65B486A1);
    }

    #[ignore]
    #[test]
    fn compress_list() {
        const DIR: &str = "../assets/filelist_raw";
        const OUT: &str = "../assets/filelist";

        let out_dir = Path::new(OUT);

        if !out_dir.exists() {
            std::fs::create_dir_all(out_dir).unwrap();
        }

        // read dir
        for entry in std::fs::read_dir(DIR).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_file() && path.extension().unwrap_or_default() == ".list" {
                let content = std::fs::read(&path).unwrap();
                let mut encoder = zstd::Encoder::new(Vec::new(), 11).unwrap();
                encoder.write_all(&content).unwrap();
                let compressed = encoder.finish().unwrap();

                let new_path = out_dir.join(path.file_name().unwrap());
                let mut new_path = new_path.to_string_lossy().to_string();
                new_path.push_str(".zst");
                std::fs::write(new_path, &compressed).unwrap();
            }
        }
    }
}

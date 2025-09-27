use std::{collections::HashMap, io::Read, path::Path};

use nohash::BuildNoHashHasher;
use parking_lot::Mutex;

use crate::{
    error::{PakError, Result},
    utf16_hash::{Utf16HashExt, Utf16LeString},
};

#[derive(Debug, Clone, Default)]
pub struct FileNameTable {
    file_names: HashMap<u64, Utf16LeString, BuildNoHashHasher<u64>>,
}

impl FileNameTable {
    pub fn file_names(&self) -> impl Iterator<Item = (&u64, &Utf16LeString)> {
        self.file_names.iter()
    }

    pub fn from_list_file<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let content = std::fs::read(path.as_ref())?;
        Self::from_bytes(&content)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let file_names = Self::parse_raw_file_names(bytes)?;
        let iter = file_names.lines().filter_map(|line| {
            if line.starts_with('#') {
                None
            } else {
                Some(line.to_string())
            }
        });

        Self::from_list(iter)
    }

    pub fn from_list(file_names: impl IntoIterator<Item = String>) -> Result<Self> {
        let this = Mutex::new(Self::default());
        file_names.into_iter().for_each(|line| {
            let file_name = Utf16LeString::new_from_str(&line.replace('\\', "/"));
            let hash = file_name.hash_mixed();
            this.lock().file_names.insert(hash, file_name);
        });

        Ok(this.into_inner())
    }

    pub fn push_str(&mut self, file_name: &str) {
        let file_name = Utf16LeString::new_from_str(&file_name.replace('\\', "/"));
        let hash = file_name.hash_mixed();
        self.file_names.insert(hash, file_name);
    }

    pub fn get_file_name(&self, hash: u64) -> Option<&Utf16LeString> {
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

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

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
            if path.is_file() && path.extension().unwrap_or_default() == "list" {
                eprintln!("path: {}", path.display());
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

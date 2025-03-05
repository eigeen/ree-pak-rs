use std::{
    cmp,
    io::{self, Read, Seek, Write},
};

use indexmap::IndexMap;

use crate::{
    filename::FileName,
    pak::{CompressionType, EncryptionType, PakEntry, PakHeader, UnkAttr},
    spec,
};

type Result<T> = std::result::Result<T, PakWriteError>;

#[derive(Debug, thiserror::Error)]
pub enum PakWriteError {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("unsupported pak version {major}.{minor}")]
    UnsupportedVersion { major: u8, minor: u8 },
}

pub struct PakWriter<W: Write + Seek> {
    pub(crate) inner: W,
    pub(crate) files: IndexMap<String, PakEntry>,
    pub(crate) pak_options: PakOptions,
    pub(crate) writing_to_file: bool,
    pub(crate) stats: PakWriterStats,
}

impl<W: Write + Seek + Read> PakWriter<W> {
    pub fn new(inner: W, alloc_entry_count: u64) -> Self {
        Self::new_with_options(
            inner,
            PakOptions {
                pre_allocate_entry_count: alloc_entry_count,
                ..Default::default()
            },
        )
        .unwrap()
    }

    pub fn new_with_options(inner: W, options: PakOptions) -> Result<Self> {
        let mut this = PakWriter {
            inner,
            files: IndexMap::new(),
            pak_options: options,
            writing_to_file: false,
            stats: PakWriterStats::default(),
        };
        this.start_pak()?;
        Ok(this)
    }

    pub fn start_file(&mut self, name: FileName, options: FileOptions) -> Result<()> {
        // finish current file
        self.try_finish_file()?;
        // create a new PakEntry
        let entry = PakEntry {
            hash_name_lower: name.hash_lower_case(),
            hash_name_upper: name.hash_upper_case(),
            offset: self.get_next_file_offset(),
            compressed_size: 0,
            uncompressed_size: 0,
            compression_type: options.compression_type,
            encryption_type: options.encryption_type,
            checksum: options.checksum,
            unk_attr: options.unk_attr,
        };
        self.inner.seek(io::SeekFrom::Start(entry.offset))?;

        self.files.insert(name.get_name().to_string(), entry);
        self.writing_to_file = true;
        Ok(())
    }

    pub fn finish(mut self) -> Result<u64> {
        if self.writing_to_file {
            self.try_finish_file()?;
        }

        self.inner.seek(io::SeekFrom::Start(0))?;
        // write toc
        let header = PakHeader {
            major_version: self.pak_options.major_version,
            minor_version: self.pak_options.minor_version,
            feature: 0,
            total_files: self.files.len() as u32,
            hash: 0,
            unk_u32_sig: 0,
            ..Default::default()
        };
        self.inner.write_all(&header.into_bytes())?;
        // ensure the space is enough to write entries,
        // or we need to move the written file data back
        // to create more header spaces
        let actual_header_size = self.calculate_header_size(self.files.len() as u64);
        match actual_header_size.cmp(&self.stats.alloc_header_size) {
            cmp::Ordering::Less => {
                // move front file data
                println!("Warning: header size is less than expected, need to move file data front.");
                let diff = self.stats.alloc_header_size - actual_header_size;
                for entry in self.files.values_mut() {
                    // move data
                    self.inner.seek(io::SeekFrom::Start(entry.offset))?;
                    let mut data = vec![0; entry.compressed_size as usize];
                    self.inner.read_exact(&mut data)?;
                    self.inner.seek(io::SeekFrom::Start(entry.offset - diff))?;
                    self.inner.write_all(&data)?;
                    // update offset
                    entry.offset -= diff;
                }
            }
            cmp::Ordering::Equal => {}
            cmp::Ordering::Greater => {
                println!("Warning: header size is larger than expected, need to move file data back.");
                // move back file data
                let diff = actual_header_size - self.stats.alloc_header_size;
                for entry in self.files.values_mut().rev() {
                    // move data
                    self.inner.seek(io::SeekFrom::Start(entry.offset))?;
                    let mut data = vec![0; entry.compressed_size as usize];
                    self.inner.read_exact(&mut data)?;
                    self.inner.seek(io::SeekFrom::Start(entry.offset + diff))?;
                    self.inner.write_all(&data)?;
                    // update offset
                    entry.offset += diff;
                }
            }
        }
        // write entries
        self.inner.seek(io::SeekFrom::Start(spec::Header::SIZE as u64))?;
        for entry in self.files.values().cloned() {
            self.inner.write_all(&entry.into_bytes_v2())?;
        }

        Ok(self.files.len() as u64)
    }

    fn start_pak(&mut self) -> Result<()> {
        if ![4].contains(&self.pak_options.major_version) || ![0].contains(&self.pak_options.minor_version) {
            return Err(PakWriteError::UnsupportedVersion {
                major: self.pak_options.major_version,
                minor: self.pak_options.minor_version,
            });
        }

        let alloc_header_size = self.calculate_header_size(self.pak_options.pre_allocate_entry_count);
        self.stats.alloc_header_size = alloc_header_size;
        Ok(())
    }

    fn try_finish_file(&mut self) -> Result<()> {
        if !self.writing_to_file {
            return Ok(());
        }
        // update stats to entry
        let entry = self.files.values_mut().last().unwrap();
        entry.uncompressed_size = self.stats.bytes_written;
        entry.compressed_size = self.stats.bytes_written;

        self.stats.reset();
        self.writing_to_file = false;
        Ok(())
    }

    fn calculate_header_size(&self, entry_count: u64) -> u64 {
        spec::Header::SIZE as u64 + entry_count * spec::EntryV2::SIZE as u64
    }

    fn get_last_file(&self) -> Option<&PakEntry> {
        self.files.values().last()
    }

    fn get_next_file_offset(&self) -> u64 {
        let last_file = self.get_last_file();
        let data_seg_offset = last_file.map(|f| f.offset + f.compressed_size).unwrap_or(0);
        let header_size = self.stats.alloc_header_size;
        header_size + data_seg_offset
    }
}

impl<W: Write + Seek> Write for PakWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if !self.writing_to_file {
            return Err(io::Error::new(io::ErrorKind::Other, "No file has been started"));
        }
        if buf.is_empty() {
            return Ok(0);
        }
        let count = self.inner.write(buf)?;
        self.stats.update(&buf[..count]);

        Ok(count)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl<W: Write + Seek> Drop for PakWriter<W> {
    fn drop(&mut self) {
        if self.writing_to_file {
            panic!("PakWriter dropped without calling finish()");
        }
    }
}

#[derive(Debug, Default)]
pub struct PakWriterStats {
    start: u64,
    bytes_written: u64,
    alloc_header_size: u64,
}

impl PakWriterStats {
    fn update(&mut self, buf: &[u8]) {
        self.bytes_written += buf.len() as u64;
    }

    fn reset(&mut self) {
        self.start = 0;
        self.bytes_written = 0;
    }
}

pub struct PakOptions {
    pub(crate) major_version: u8,
    pub(crate) minor_version: u8,
    pub(crate) toc_hash: u32,
    pub(crate) pre_allocate_entry_count: u64,
}

impl Default for PakOptions {
    fn default() -> Self {
        Self {
            major_version: 4,
            minor_version: 0,
            toc_hash: 0,
            pre_allocate_entry_count: 0,
        }
    }
}

impl PakOptions {
    pub fn with_version(mut self, major_version: u8, minor_version: u8) -> Self {
        self.major_version = major_version;
        self.minor_version = minor_version;
        self
    }

    pub fn with_toc_hash(mut self, toc_hash: u32) -> Self {
        self.toc_hash = toc_hash;
        self
    }

    pub fn with_pre_allocate_entry_count(mut self, pre_allocate_entry_count: u64) -> Self {
        self.pre_allocate_entry_count = pre_allocate_entry_count;
        self
    }
}

#[derive(Debug, Default)]
pub struct FileOptions {
    pub(crate) compression_type: CompressionType,
    pub(crate) encryption_type: EncryptionType,
    pub(crate) checksum: u64,
    pub(crate) unk_attr: UnkAttr,
}

impl FileOptions {
    pub fn with_compression_type(mut self, compression_type: CompressionType) -> Self {
        self.compression_type = compression_type;
        self
    }

    pub fn with_encryption_type(mut self, encryption_type: EncryptionType) -> Self {
        self.encryption_type = encryption_type;
        self
    }

    pub fn with_checksum(mut self, checksum: u64) -> Self {
        self.checksum = checksum;
        self
    }

    pub fn with_unk_attr(mut self, unk_attr: UnkAttr) -> Self {
        self.unk_attr = unk_attr;
        self
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::read;

    use super::*;

    #[test]
    fn test_pak_writer() {
        let mut vec = vec![];
        let buf = Cursor::new(&mut vec);
        let mut writer = PakWriter::new(buf, 1);
        writer.start_file("test.txt".into(), FileOptions::default()).unwrap();
        writer.write_all(b"hello world").unwrap();
        writer.start_file("a/test2.txt".into(), FileOptions::default()).unwrap();
        writer.write_all("你好，中国！".as_bytes()).unwrap();
        writer.finish().unwrap();

        println!("{:?}", vec);

        let mut reader = Cursor::new(vec);
        let archive = read::read_archive(&mut reader).unwrap();
        println!("{:#?}", archive);
    }
}

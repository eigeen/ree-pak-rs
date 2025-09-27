use std::io::{self, Seek, Write};

use indexmap::IndexMap;

use crate::{
    pak::{CompressionType, EncryptionType, FeatureFlags, PakEntry, PakHeader, UnkAttr},
    spec,
    utf16_hash::Utf16HashExt,
};

type Result<T> = std::result::Result<T, PakWriteError>;

#[derive(Debug, thiserror::Error)]
pub enum PakWriteError {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("unsupported pak version {major}.{minor}")]
    UnsupportedVersion { major: u8, minor: u8 },
    #[error("entry count exceeded the pre-allocated count.")]
    EntryCountExceeded,
}

pub struct PakWriter<W> {
    pub(crate) inner: W,
    pub(crate) files: IndexMap<u64, PakEntry>,
    pub(crate) pak_options: PakOptions,
    pub(crate) writing_to_file: bool,
    pub(crate) stats: PakWriterStats,
}

impl<W: Write + Seek> PakWriter<W> {
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

    pub fn start_file(&mut self, path: impl Utf16HashExt, options: FileOptions) -> Result<()> {
        self.start_file_hash(path.hash_mixed(), options)
    }

    pub fn start_file_hash(&mut self, hash: u64, options: FileOptions) -> Result<()> {
        // finish current file
        self.try_finish_file()?;
        if self.files.len() >= self.pak_options.pre_allocate_entry_count as usize {
            return Err(PakWriteError::EntryCountExceeded);
        }
        // create a new PakEntry
        let entry = PakEntry {
            hash_name_lower: hash.hash_lower_case(),
            hash_name_upper: hash.hash_upper_case(),
            offset: self.inner.stream_position()?,
            compressed_size: 0,
            uncompressed_size: 0,
            compression_type: options.compression_type,
            encryption_type: options.encryption_type,
            checksum: options.checksum,
            unk_attr: options.unk_attr,
        };

        self.files.insert(hash, entry);
        self.writing_to_file = true;
        Ok(())
    }

    pub fn finish(mut self) -> Result<u64> {
        if self.writing_to_file {
            self.try_finish_file()?;
        }
        // check if entry count is correct
        if self.files.len() as u32 != self.pak_options.pre_allocate_entry_count as u32 {
            eprintln!("Warning: the actual file count is less than the pre-allocated count. It may cause space waste.");
        }

        self.inner.seek(io::SeekFrom::Start(0))?;
        // write toc
        let header = PakHeader {
            major_version: self.pak_options.major_version,
            minor_version: self.pak_options.minor_version,
            feature: FeatureFlags::default(),
            total_files: self.files.len() as u32,
            hash: 0,
            unk_u32_sig: 0,
            ..Default::default()
        };
        self.inner.write_all(&header.into_bytes())?;
        // write entries
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
        self.inner.seek(io::SeekFrom::Start(alloc_header_size))?;
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
}

impl<W: Write> Write for PakWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if !self.writing_to_file {
            return Err(io::Error::other("No file has been started"));
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

impl<W> Drop for PakWriter<W> {
    fn drop(&mut self) {
        if self.writing_to_file {
            panic!("PakWriter dropped without calling finish()");
        }
    }
}

#[derive(Debug, Default)]
pub struct PakWriterStats {
    bytes_written: u64,
}

impl PakWriterStats {
    fn update(&mut self, buf: &[u8]) {
        self.bytes_written += buf.len() as u64;
    }

    fn reset(&mut self) {
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
    use std::io::{Cursor, Read};

    use crate::read::{self, archive::PakArchiveReader};

    use super::*;

    #[test]
    fn test_pak_writer() {
        let mut vec = vec![];
        let buf = Cursor::new(&mut vec);
        let mut writer = PakWriter::new(buf, 2);
        writer.start_file("test.txt", FileOptions::default()).unwrap();
        writer.write_all(b"hello world").unwrap();
        writer.start_file("a/test2.txt", FileOptions::default()).unwrap();
        writer.write_all("你好，中国！".as_bytes()).unwrap();
        writer.finish().unwrap();

        println!("{:?}", vec);

        let mut reader = Cursor::new(vec);
        let archive = read::read_archive(&mut reader).unwrap();
        println!("{:#?}", archive);
        let mut archive_reader = PakArchiveReader::new(reader, &archive);
        for (i, entry) in archive.entries().iter().enumerate() {
            let mut entry_reader = archive_reader.owned_entry_reader(entry.clone()).unwrap();
            let mut buf = vec![0; entry.uncompressed_size as usize];
            entry_reader.read_exact(&mut buf).unwrap();
            if i == 0 {
                assert_eq!(buf, b"hello world");
            } else if i == 1 {
                assert_eq!(buf, "你好，中国！".as_bytes());
            }
        }
    }
}

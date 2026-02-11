use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use memmap2::{Mmap, MmapOptions};

use crate::error::{PakError, Result};
use crate::pak::{PakArchive, PakEntry};
use crate::read::{self, entry::PakEntryReader};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PakBackend {
    /// Use `memmap2` memory mapping.
    Mmap,
    /// Use regular file IO.
    File,
}

impl Default for PakBackend {
    fn default() -> Self {
        Self::Mmap
    }
}

#[derive(Debug, Default)]
pub struct PakFileBuilder {
    backend: PakBackend,
}

impl PakFileBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn backend(mut self, backend: PakBackend) -> Self {
        self.backend = backend;
        self
    }

    pub fn mmap(mut self, enabled: bool) -> Self {
        self.backend = if enabled { PakBackend::Mmap } else { PakBackend::File };
        self
    }

    pub fn open(self, path: impl AsRef<Path>) -> Result<PakFile> {
        PakFile::open_with_backend(path, self.backend)
    }
}

/// High-level, parallel-friendly pak file handle.
pub struct PakFile {
    path: PathBuf,
    archive: PakArchive,
    backend: PakBackend,
    inner: PakFileInner,
}

enum PakFileInner {
    Mmap { mmap: Arc<Mmap> },
    File { file: File },
}

impl PakFile {
    pub fn builder() -> PakFileBuilder {
        PakFileBuilder::new()
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_backend(path, PakBackend::default())
    }

    pub fn open_with_backend(path: impl AsRef<Path>, backend: PakBackend) -> Result<Self> {
        let path = path.as_ref();
        let path_abs = path
            .canonicalize()
            .map_err(|e| PakError::IO(std::io::Error::new(e.kind(), format!("{}: {}", path.display(), e))))?;

        let file = File::open(&path_abs)?;
        let mut reader = BufReader::new(file);
        let archive = read::read_archive(&mut reader)?;

        let file = reader.into_inner();

        let inner = match backend {
            PakBackend::Mmap => {
                // SAFETY: read-only mapping; the file is held for the lifetime of the mmap.
                let mmap = unsafe { MmapOptions::new().map(&file)? };
                PakFileInner::Mmap { mmap: Arc::new(mmap) }
            }
            PakBackend::File => PakFileInner::File { file },
        };

        Ok(Self {
            path: path_abs,
            archive,
            backend,
            inner,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn archive(&self) -> &PakArchive {
        &self.archive
    }

    pub fn backend(&self) -> PakBackend {
        self.backend
    }

    pub fn open_entry(&self, entry: &PakEntry) -> Result<PakEntryReader<Box<dyn BufRead + Send>>> {
        let raw: Box<dyn BufRead + Send> = match &self.inner {
            PakFileInner::Mmap { mmap } => {
                let offset = entry.offset() as usize;
                let len = entry.compressed_size() as usize;
                let end = offset.saturating_add(len);
                if end > mmap.len() {
                    return Err(PakError::InvalidEntryRange {
                        offset: entry.offset(),
                        size: entry.compressed_size(),
                        file_size: mmap.len() as u64,
                    });
                }
                Box::new(MmapRangeReader::new(Arc::clone(mmap), offset, end))
            }
            PakFileInner::File { file } => {
                let mut f = file.try_clone()?;
                f.seek(SeekFrom::Start(entry.offset()))?;
                let take = f.take(entry.compressed_size());
                Box::new(BufReader::new(take))
            }
        };

        PakEntryReader::new_boxed(raw, entry.clone())
    }
}

struct MmapRangeReader {
    mmap: Arc<Mmap>,
    end: usize,
    pos: usize,
}

impl MmapRangeReader {
    fn new(mmap: Arc<Mmap>, start: usize, end: usize) -> Self {
        Self { mmap, end, pos: start }
    }
}

impl Read for MmapRangeReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let remaining = self.end.saturating_sub(self.pos);
        if remaining == 0 {
            return Ok(0);
        }
        let to_read = remaining.min(buf.len());
        let src = &self.mmap[self.pos..self.pos + to_read];
        buf[..to_read].copy_from_slice(src);
        self.pos += to_read;
        Ok(to_read)
    }
}

impl BufRead for MmapRangeReader {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        Ok(&self.mmap[self.pos..self.end])
    }

    fn consume(&mut self, amt: usize) {
        self.pos = (self.pos + amt).min(self.end);
    }
}

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use memmap2::{Mmap, MmapOptions};

use crate::error::{PakError, Result};
use crate::pak::{EntryOffset, FeatureFlags, PakArchive, PakEntry};
use crate::read::chunk_table::ChunkTable;
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
    chunk_table: Option<Arc<ChunkTable>>,
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
        let chunk_table = if archive.header().feature().contains(FeatureFlags::CHUNK_TABLE) {
            Some(Arc::new(read::chunk_table::read_chunk_table(&mut reader)?))
        } else {
            None
        };

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
            chunk_table,
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
        let raw: Box<dyn BufRead + Send> = match entry.offset() {
            EntryOffset::ChunkIndex(chunk_index) => {
                let table = self.chunk_table.as_ref().ok_or(PakError::MissingChunkTable)?;
                let start_chunk = usize::try_from(chunk_index).map_err(|_| PakError::InvalidChunkIndex(chunk_index))?;
                // Chunked entries are compressed per-chunk (or stored raw) and expanded by `ChunkedRead`.
                // The entry's `compressed_size` is not the byte length produced by the chunk reader.
                let total_len = if entry.uncompressed_size() != 0 {
                    entry.uncompressed_size()
                } else {
                    entry.compressed_size()
                };
                match &self.inner {
                    PakFileInner::Mmap { mmap } => Box::new(BufReader::new(ChunkedRead::new_mmap(
                        Arc::clone(mmap),
                        Arc::clone(table),
                        start_chunk,
                        total_len,
                    )?)),
                    PakFileInner::File { file } => Box::new(BufReader::new(ChunkedRead::new_file(
                        file.try_clone()?,
                        Arc::clone(table),
                        start_chunk,
                        total_len,
                    )?)),
                }
            }
            EntryOffset::FileOffset(file_offset) => match &self.inner {
                PakFileInner::Mmap { mmap } => {
                    let offset = file_offset as usize;
                    let len = entry.compressed_size() as usize;
                    let end = offset.saturating_add(len);
                    if end > mmap.len() {
                        return Err(PakError::InvalidEntryRange {
                            offset: file_offset,
                            size: entry.compressed_size(),
                            file_size: mmap.len() as u64,
                        });
                    }
                    Box::new(MmapRangeReader::new(Arc::clone(mmap), offset, end))
                }
                PakFileInner::File { file } => {
                    let mut f = file.try_clone()?;
                    f.seek(SeekFrom::Start(file_offset))?;
                    let take = f.take(entry.compressed_size());
                    Box::new(BufReader::new(take))
                }
            },
        };

        PakEntryReader::new_boxed(raw, entry.clone())
    }
}

enum ChunkedSource {
    Mmap { mmap: Arc<Mmap> },
    File { file: File },
}

struct ChunkedRead {
    source: ChunkedSource,
    table: Arc<ChunkTable>,
    next_chunk_index: usize,
    remaining: u64,
    buf: Vec<u8>,
    buf_pos: usize,
}

impl ChunkedRead {
    fn new_mmap(mmap: Arc<Mmap>, table: Arc<ChunkTable>, start_chunk: usize, total_len: u64) -> Result<Self> {
        Self::new(ChunkedSource::Mmap { mmap }, table, start_chunk, total_len)
    }

    fn new_file(file: File, table: Arc<ChunkTable>, start_chunk: usize, total_len: u64) -> Result<Self> {
        Self::new(ChunkedSource::File { file }, table, start_chunk, total_len)
    }

    fn new(source: ChunkedSource, table: Arc<ChunkTable>, start_chunk: usize, total_len: u64) -> Result<Self> {
        let block_size = table.block_size() as u64;
        if block_size == 0 {
            return Err(PakError::InvalidChunkTable("block_size is 0"));
        }
        if start_chunk >= table.chunks().len() {
            return Err(PakError::InvalidChunkIndex(start_chunk as u64));
        }

        // Best-effort bounds check: ensure we have enough chunks to cover the declared length.
        if total_len > 0 {
            let needed = (total_len + block_size - 1) / block_size;
            let end = start_chunk.saturating_add(needed as usize);
            if end > table.chunks().len() {
                return Err(PakError::InvalidChunkIndex(end as u64));
            }
        }

        Ok(Self {
            source,
            table,
            next_chunk_index: start_chunk,
            remaining: total_len,
            buf: Vec::new(),
            buf_pos: 0,
        })
    }

    fn refill(&mut self) -> std::io::Result<()> {
        if self.remaining == 0 {
            self.buf.clear();
            self.buf_pos = 0;
            return Ok(());
        }

        let desc = self
            .table
            .chunks()
            .get(self.next_chunk_index)
            .ok_or_else(|| std::io::Error::other(format!("chunk index out of range: {}", self.next_chunk_index)))?
            .clone();
        self.next_chunk_index += 1;

        let block_size = self.table.block_size() as usize;
        let comp_len = desc.compressed_len(self.table.block_size()) as usize;
        let start = desc.start() as usize;
        let end = start.saturating_add(comp_len);

        let comp_bytes = match &mut self.source {
            ChunkedSource::Mmap { mmap } => {
                if end > mmap.len() {
                    return Err(std::io::Error::other(format!(
                        "chunk range out of bounds: start={start} end={end} file_size={}",
                        mmap.len()
                    )));
                }
                mmap[start..end].to_vec()
            }
            ChunkedSource::File { file } => {
                file.seek(SeekFrom::Start(desc.start()))?;
                let mut buf = vec![0u8; comp_len];
                file.read_exact(&mut buf)?;
                buf
            }
        };

        let out = if desc.is_raw() {
            comp_bytes
        } else {
            zstd::stream::decode_all(std::io::Cursor::new(comp_bytes)).map_err(|e| {
                std::io::Error::other(format!(
                    "zstd decode failed at chunk {}: {}",
                    self.next_chunk_index - 1,
                    e
                ))
            })?
        };

        if out.len() != block_size {
            return Err(std::io::Error::other(format!(
                "unexpected chunk output size at chunk {}: got {} expected {}",
                self.next_chunk_index - 1,
                out.len(),
                block_size
            )));
        }

        self.buf = out;
        self.buf_pos = 0;
        Ok(())
    }
}

impl Read for ChunkedRead {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }

        if self.buf_pos >= self.buf.len() {
            self.refill()?;
            if self.buf.is_empty() {
                return Ok(0);
            }
        }

        let available = self.buf.len().saturating_sub(self.buf_pos);
        let want = out.len().min(available).min(self.remaining as usize);
        out[..want].copy_from_slice(&self.buf[self.buf_pos..self.buf_pos + want]);
        self.buf_pos += want;
        self.remaining -= want as u64;
        Ok(want)
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

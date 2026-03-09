use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::sync::Arc;

use memmap2::Mmap;

use crate::error::{PakError, Result};
use crate::pak::{ChunkCompressionType, EntryOffset, FeatureFlags, PakEntry, PakMetadata};
use crate::read::chunk_table::ChunkTable;
use crate::read::{self, PakReadOptions, entry::PakEntryReader};

/// A pak reader that can be cheaply cloned for independent seeking/reading.
///
/// [`PakFile`] uses `try_clone()` to open multiple entries concurrently (e.g. parallel extraction),
/// so the underlying reader must support independent cursors.
///
/// - `std::fs::File` does **not** provide independent cursors when cloned at the OS-handle level (`try_clone` shares
///   the underlying file pointer). This crate avoids that by using memory-mapped I/O for file-backed readers.
/// - In-memory readers like `std::io::Cursor<Vec<u8>>` are `Clone`, and therefore implement [`PakReader`]
///   via the blanket impl below.
pub trait PakReader: Read + Seek + Send + Sync {
    /// Clone this reader so the clone can seek/read independently.
    fn try_clone(&self) -> std::io::Result<Self>
    where
        Self: Sized;
}

impl<T> PakReader for T
where
    T: Read + Seek + Clone + Send + Sync,
{
    fn try_clone(&self) -> std::io::Result<Self> {
        Ok(self.clone())
    }
}

/// A read-only `File` mapped into memory.
///
/// Clones share the mapping but keep an independent cursor.
#[derive(Debug)]
pub struct MmapFile {
    mmap: Arc<Mmap>,
    pos: u64,
}

impl MmapFile {
    /// Map an opened file into memory (read-only).
    pub fn new(file: &File) -> std::io::Result<Self> {
        // SAFETY: The mapping is treated as immutable. If the underlying file is concurrently modified,
        // the OS may expose inconsistent data; callers must ensure the pak file is not mutated while mapped.
        let mmap = unsafe { Mmap::map(file)? };
        Ok(Self {
            mmap: Arc::new(mmap),
            pos: 0,
        })
    }

    fn len(&self) -> u64 {
        self.mmap.len() as u64
    }
}

impl Read for MmapFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let len = self.len();
        if self.pos >= len || buf.is_empty() {
            return Ok(0);
        }

        let start = self.pos as usize;
        let available = self.mmap.len().saturating_sub(start);
        let want = buf.len().min(available);
        buf[..want].copy_from_slice(&self.mmap[start..start + want]);
        self.pos = self.pos.saturating_add(want as u64);
        Ok(want)
    }
}

impl Seek for MmapFile {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let len = self.len() as i128;
        let new_pos: i128 = match pos {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::Current(delta) => self.pos as i128 + delta as i128,
            SeekFrom::End(delta) => len + delta as i128,
        };

        if new_pos < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid seek to a negative position",
            ));
        }

        self.pos = new_pos as u64;
        Ok(self.pos)
    }
}

impl PakReader for MmapFile {
    fn try_clone(&self) -> std::io::Result<Self> {
        Ok(Self {
            mmap: Arc::clone(&self.mmap),
            pos: self.pos,
        })
    }
}

/// A `File` wrapper that implements [`PakReader`] with independent cursors.
///
/// This is the default reader type for [`PakFile`], so `PakFile::from_file(File)` “just works”.
#[derive(Debug)]
pub struct CloneableFile {
    mmap: Arc<Mmap>,
    pos: u64,
}

impl CloneableFile {
    /// Wrap a `std::fs::File`.
    pub fn new(file: File) -> std::io::Result<Self> {
        // SAFETY: The mapping is treated as immutable. If the underlying file is concurrently modified,
        // the OS may expose inconsistent data; callers must ensure the pak file is not mutated while mapped.
        let mmap = unsafe { Mmap::map(&file)? };
        Ok(Self {
            mmap: Arc::new(mmap),
            pos: 0,
        })
    }
}

impl Read for CloneableFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let len = self.mmap.len() as u64;
        if self.pos >= len || buf.is_empty() {
            return Ok(0);
        }

        let start = self.pos as usize;
        let available = self.mmap.len().saturating_sub(start);
        let want = buf.len().min(available);
        buf[..want].copy_from_slice(&self.mmap[start..start + want]);
        self.pos = self.pos.saturating_add(want as u64);
        Ok(want)
    }
}

impl Seek for CloneableFile {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let len = self.mmap.len() as i128;
        let new_pos: i128 = match pos {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::Current(delta) => self.pos as i128 + delta as i128,
            SeekFrom::End(delta) => len + delta as i128,
        };

        if new_pos < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid seek to a negative position",
            ));
        }

        self.pos = new_pos as u64;
        Ok(self.pos)
    }
}

impl PakReader for CloneableFile {
    fn try_clone(&self) -> std::io::Result<Self> {
        Ok(Self {
            mmap: Arc::clone(&self.mmap),
            pos: self.pos,
        })
    }
}

/// High-level, parallel-friendly pak file handle.
///
/// ## Design notes
///
/// - Construction reads and caches [`PakMetadata`] (header + entry table).
/// - If the pak header contains [`FeatureFlags::CHUNK_TABLE`], the chunk table is loaded and stored.
/// - Entry reads are performed via fresh clones of the underlying reader (`R: PakReader`),
///   enabling parallel extraction without sharing a single `&mut R`.
///
/// Use [`PakFile::open_entry`] to obtain a [`PakEntryReader`] that transparently decrypts and decompresses
/// the entry payload.
pub struct PakFile<R: PakReader = CloneableFile> {
    reader: R,
    metadata: PakMetadata,
    chunk_table: Option<Arc<ChunkTable>>,
    file_size: Option<u64>,
}

impl PakFile<CloneableFile> {
    /// Create a [`PakFile`] from a `std::fs::File`.
    pub fn from_file(file: File) -> Result<Self> {
        Self::from_reader(CloneableFile::new(file)?)
    }

    /// Create a [`PakFile`] from a `std::fs::File` using custom options.
    pub fn from_file_with_options(file: File, options: PakReadOptions) -> Result<Self> {
        Self::from_reader_with_options(CloneableFile::new(file)?, options)
    }

    /// Create a [`PakFile`] by memory-mapping the file (read-only).
    pub fn from_file_mmap(file: &File) -> Result<PakFile<MmapFile>> {
        PakFile::from_reader(MmapFile::new(file)?)
    }
}

impl<R> PakFile<R>
where
    R: PakReader,
{
    /// Create a [`PakFile`] from a custom reader.
    ///
    /// This reads metadata from the start of the stream and keeps `reader` for later entry access.
    /// The reader will be cloned internally to perform independent seeks.
    pub fn from_reader(reader: R) -> Result<Self> {
        Self::from_reader_with_options(reader, PakReadOptions::default())
    }

    /// Create a [`PakFile`] from a custom reader using custom options.
    pub fn from_reader_with_options(reader: R, options: PakReadOptions) -> Result<Self> {
        let file_size = {
            let mut r = reader.try_clone()?;
            r.seek(SeekFrom::End(0)).ok()
        };

        let (metadata, chunk_table) = {
            let mut r = reader.try_clone()?;
            r.seek(SeekFrom::Start(0))?;
            let mut buf = BufReader::new(r);
            let metadata = read::read_metadata_with_options(&mut buf, options)?;
            let chunk_table = if metadata.header().feature().contains(FeatureFlags::CHUNK_TABLE) {
                Some(Arc::new(read::chunk_table::read_chunk_table(&mut buf)?))
            } else {
                None
            };
            (metadata, chunk_table)
        };

        Ok(Self {
            reader,
            metadata,
            chunk_table,
            file_size,
        })
    }

    /// Return the cached pak metadata (header + entry table).
    pub fn metadata(&self) -> &PakMetadata {
        &self.metadata
    }

    /// Open an entry for reading (decrypt + decompress).
    ///
    /// This API supports both kinds of entry offsets:
    /// - byte offsets (`EntryOffset::FileOffset`)
    /// - chunk-index offsets (`EntryOffset::ChunkIndex`, requires a loaded chunk table)
    ///
    /// The returned reader implements `Read` and will stream the decompressed bytes.
    /// For encrypted entries, the compressed bytes are fully buffered in memory in order to decrypt.
    pub fn open_entry(&self, entry: &PakEntry) -> Result<PakEntryReader<Box<dyn BufRead + Send + '_>>> {
        let raw: Box<dyn BufRead + Send + '_> = match entry.offset() {
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
                let r = self.reader.try_clone()?;
                Box::new(BufReader::new(ChunkedRead::new(
                    r,
                    self.file_size,
                    Arc::clone(table),
                    start_chunk,
                    total_len,
                )?))
            }
            EntryOffset::FileOffset(file_offset) => {
                if let Some(file_size) = self.file_size {
                    let end = file_offset.saturating_add(entry.compressed_size());
                    if end > file_size {
                        return Err(PakError::InvalidEntryRange {
                            offset: file_offset,
                            size: entry.compressed_size(),
                            file_size,
                        });
                    }
                }
                let mut r = self.reader.try_clone()?;
                r.seek(SeekFrom::Start(file_offset))?;
                let take = r.take(entry.compressed_size());
                Box::new(BufReader::new(take))
            }
        };

        PakEntryReader::new_boxed(raw, entry.clone())
    }
}

struct ChunkedRead<R> {
    reader: R,
    file_size: Option<u64>,
    table: Arc<ChunkTable>,
    next_chunk_index: usize,
    remaining: u64,
    buf: Vec<u8>,
    buf_pos: usize,
}

#[derive(Debug, thiserror::Error)]
#[error("failed to decode chunk {chunk_index} (compression={compression:?}, start={start}, end={end}): {kind}")]
struct ChunkDecodeError {
    chunk_index: usize,
    compression: ChunkCompressionType,
    start: u64,
    end: u64,
    #[source]
    kind: ChunkDecodeErrorKind,
}

#[derive(Debug, thiserror::Error)]
enum ChunkDecodeErrorKind {
    #[error("chunk range out of bounds (file_size={file_size})")]
    OutOfBounds { file_size: u64 },
    #[error("failed to read chunk bytes")]
    Read(#[source] std::io::Error),
    #[error("zstd decode failed")]
    Zstd(#[source] std::io::Error),
    #[error("unexpected chunk output size (got={got}, expected={expected})")]
    OutputSize { got: usize, expected: usize },
}

impl<R> ChunkedRead<R>
where
    R: Read + Seek,
{
    fn new(
        reader: R,
        file_size: Option<u64>,
        table: Arc<ChunkTable>,
        start_chunk: usize,
        total_len: u64,
    ) -> Result<Self> {
        let block_size = table.block_size() as u64;
        if block_size == 0 {
            return Err(PakError::InvalidChunkTable("block_size is 0".to_string()));
        }
        if start_chunk >= table.chunks().len() {
            return Err(PakError::InvalidChunkIndex(start_chunk as u64));
        }

        // Best-effort bounds check: ensure we have enough chunks to cover the declared length.
        if total_len > 0 {
            let needed = total_len.div_ceil(block_size);
            let end = start_chunk.saturating_add(needed as usize);
            if end > table.chunks().len() {
                return Err(PakError::InvalidChunkIndex(end as u64));
            }
        }

        Ok(Self {
            reader,
            file_size,
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

        let chunk_index = self.next_chunk_index;
        let desc = self
            .table
            .chunks()
            .get(chunk_index)
            .ok_or_else(|| std::io::Error::other(format!("chunk index out of range: {chunk_index}")))?
            .clone();
        self.next_chunk_index += 1;

        let block_size = self.table.block_size() as usize;
        let block_size_u32 = self.table.block_size();
        let compression = desc.compression_type(block_size_u32);
        let comp_len = desc.compressed_len() as usize;
        let start = desc.start();
        let end = start.saturating_add(comp_len as u64);

        if let Some(file_size) = self.file_size
            && end > file_size
        {
            return Err(std::io::Error::other(ChunkDecodeError {
                chunk_index,
                compression,
                start,
                end,
                kind: ChunkDecodeErrorKind::OutOfBounds { file_size },
            }));
        }

        self.reader.seek(SeekFrom::Start(start))?;
        let mut comp_bytes = vec![0u8; comp_len];
        self.reader.read_exact(&mut comp_bytes).map_err(|e| {
            std::io::Error::new(
                e.kind(),
                ChunkDecodeError {
                    chunk_index,
                    compression,
                    start,
                    end,
                    kind: ChunkDecodeErrorKind::Read(e),
                },
            )
        })?;

        let out = match compression {
            ChunkCompressionType::None => comp_bytes,
            ChunkCompressionType::Zstd => {
                // Chunked extraction expects exactly one fixed-size block output. Use a capped decode
                // to avoid unbounded allocations on corrupted input.
                let decoder = zstd::Decoder::new(std::io::Cursor::new(comp_bytes)).map_err(|e| {
                    std::io::Error::new(
                        e.kind(),
                        ChunkDecodeError {
                            chunk_index,
                            compression,
                            start,
                            end,
                            kind: ChunkDecodeErrorKind::Zstd(e),
                        },
                    )
                })?;

                let mut out = Vec::with_capacity(block_size);
                decoder.take(block_size as u64 + 1).read_to_end(&mut out).map_err(|e| {
                    std::io::Error::new(
                        e.kind(),
                        ChunkDecodeError {
                            chunk_index,
                            compression,
                            start,
                            end,
                            kind: ChunkDecodeErrorKind::Zstd(e),
                        },
                    )
                })?;
                out
            }
        };

        if out.len() != block_size {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                ChunkDecodeError {
                    chunk_index,
                    compression,
                    start,
                    end,
                    kind: ChunkDecodeErrorKind::OutputSize {
                        got: out.len(),
                        expected: block_size,
                    },
                },
            ));
        }

        self.buf = out;
        self.buf_pos = 0;
        Ok(())
    }
}

impl<R> Read for ChunkedRead<R>
where
    R: Read + Seek,
{
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{SeekFrom, Write as _};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        p.push(format!("ree_pak_core_{tag}_{}_{}.bin", std::process::id(), nanos));
        p
    }

    #[test]
    fn cloneable_file_independent_cursor() {
        let path = temp_path("cloneablefile_cursor");

        {
            let mut f = File::create(&path).unwrap();
            let bytes: Vec<u8> = (0u8..=255).collect();
            f.write_all(&bytes).unwrap();
        }

        let base = CloneableFile::new(File::open(&path).unwrap()).unwrap();
        let mut a = base.try_clone().unwrap();
        let mut b = base.try_clone().unwrap();

        a.seek(SeekFrom::Start(10)).unwrap();
        b.seek(SeekFrom::Start(20)).unwrap();

        let mut one = [0u8; 1];
        a.read_exact(&mut one).unwrap();
        assert_eq!(one[0], 10);
        a.read_exact(&mut one).unwrap();
        assert_eq!(one[0], 11);

        b.read_exact(&mut one).unwrap();
        assert_eq!(one[0], 20);
        b.read_exact(&mut one).unwrap();
        assert_eq!(one[0], 21);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn mmap_file_independent_cursor() {
        let path = temp_path("mmapfile_cursor");

        {
            let mut f = File::create(&path).unwrap();
            let bytes: Vec<u8> = (0u8..=255).collect();
            f.write_all(&bytes).unwrap();
        }

        let file = File::open(&path).unwrap();
        let base = MmapFile::new(&file).unwrap();
        let mut a = base.try_clone().unwrap();
        let mut b = base.try_clone().unwrap();

        a.seek(SeekFrom::Start(10)).unwrap();
        b.seek(SeekFrom::Start(20)).unwrap();

        let mut one = [0u8; 1];
        a.read_exact(&mut one).unwrap();
        assert_eq!(one[0], 10);
        a.read_exact(&mut one).unwrap();
        assert_eq!(one[0], 11);

        b.read_exact(&mut one).unwrap();
        assert_eq!(one[0], 20);
        b.read_exact(&mut one).unwrap();
        assert_eq!(one[0], 21);

        let _ = std::fs::remove_file(&path);
    }
}

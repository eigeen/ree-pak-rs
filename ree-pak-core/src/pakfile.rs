use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::sync::Arc;

use crate::error::{PakError, Result};
use crate::pak::{CompressionType, EntryOffset, FeatureFlags, PakEntry, PakMetadata};
use crate::read::chunk_table::ChunkTable;
use crate::read::{self, entry::PakEntryReader};

pub trait PakReader: Read + Seek + Send + Sync {
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

#[derive(Debug)]
pub struct CloneableFile(File);

impl CloneableFile {
    pub fn new(file: File) -> Self {
        Self(file)
    }

    pub fn into_inner(self) -> File {
        self.0
    }
}

impl Read for CloneableFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl Seek for CloneableFile {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.0.seek(pos)
    }
}

impl PakReader for CloneableFile {
    fn try_clone(&self) -> std::io::Result<Self> {
        self.0.try_clone().map(Self)
    }
}

/// High-level, parallel-friendly pak file handle.
pub struct PakFile<R: PakReader = CloneableFile> {
    reader: R,
    metadata: PakMetadata,
    chunk_table: Option<Arc<ChunkTable>>,
    file_size: Option<u64>,
}

impl PakFile<CloneableFile> {
    pub fn from_file(file: File) -> Result<Self> {
        Self::from_reader(CloneableFile::new(file))
    }
}

impl<R> PakFile<R>
where
    R: PakReader,
{
    pub fn from_reader(reader: R) -> Result<Self> {
        let file_size = {
            let mut r = reader.try_clone()?;
            r.seek(SeekFrom::End(0)).ok()
        };

        let (metadata, chunk_table) = {
            let mut r = reader.try_clone()?;
            r.seek(SeekFrom::Start(0))?;
            let mut buf = BufReader::new(r);
            let metadata = read::read_metadata(&mut buf)?;
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

    pub fn metadata(&self) -> &PakMetadata {
        &self.metadata
    }

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

        let desc = self
            .table
            .chunks()
            .get(self.next_chunk_index)
            .ok_or_else(|| std::io::Error::other(format!("chunk index out of range: {}", self.next_chunk_index)))?
            .clone();
        self.next_chunk_index += 1;

        let block_size = self.table.block_size() as usize;
        let comp_len = desc.compressed_len(self.table.block_size()).ok_or_else(|| {
            std::io::Error::other("unknown chunk compression type, failed to get compressed length for chunk")
        })? as usize;
        let start = desc.start() as usize;
        let end = start.saturating_add(comp_len);

        if let Some(file_size) = self.file_size
            && end as u64 > file_size
        {
            return Err(std::io::Error::other(format!(
                "chunk range out of bounds: start={start} end={end} file_size={file_size}"
            )));
        }

        self.reader.seek(SeekFrom::Start(desc.start()))?;
        let mut comp_bytes = vec![0u8; comp_len];
        self.reader.read_exact(&mut comp_bytes)?;

        let ct = desc
            .compression_type()
            .ok_or_else(|| std::io::Error::other("unknown chunk compression type"))?;
        let out = match ct {
            CompressionType::None => comp_bytes,
            CompressionType::Deflate => {
                let mut decoder = flate2::bufread::DeflateDecoder::new(std::io::Cursor::new(comp_bytes));
                let mut out = Vec::new();
                decoder.read_to_end(&mut out)?;
                out
            }
            CompressionType::Zstd => zstd::stream::decode_all(std::io::Cursor::new(comp_bytes)).map_err(|e| {
                std::io::Error::other(format!(
                    "zstd decode failed at chunk {}: {}",
                    self.next_chunk_index - 1,
                    e
                ))
            })?,
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

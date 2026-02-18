use std::io::{BufRead, Read};

use crate::error::Result;
use crate::pak::CompressionType;

use super::PakReaderError;

/// Read a compressed file.
pub enum CompressedReader<R> {
    Store(R),
    Deflate(flate2::bufread::DeflateDecoder<R>),
    Zstd(zstd::Decoder<'static, R>),
}

impl<R> CompressedReader<R> {
    pub fn compression_type(&self) -> CompressionType {
        match self {
            CompressedReader::Store(_) => CompressionType::None,
            CompressedReader::Deflate(_) => CompressionType::Deflate,
            CompressedReader::Zstd(_) => CompressionType::Zstd,
        }
    }
}

impl<R> CompressedReader<R>
where
    R: BufRead,
{
    pub fn new(reader: R, compression: CompressionType) -> Result<Self> {
        match compression {
            CompressionType::None => Ok(Self::Store(reader)),
            CompressionType::Deflate => Ok(Self::Deflate(flate2::bufread::DeflateDecoder::new(reader))),
            CompressionType::Zstd => Ok(Self::Zstd(zstd::stream::Decoder::with_buffer(reader)?)),
        }
    }
}

impl<R> Read for CompressedReader<R>
where
    R: BufRead,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            CompressedReader::Store(inner) => inner.read(buf),
            CompressedReader::Deflate(inner) => inner.read(buf),
            CompressedReader::Zstd(inner) => inner.read(buf),
        }
        .map_err(|e| PakReaderError::Decompression {
            compression: self.compression_type(),
            source: e,
        })
        .map_err(|e| e.into_io_error())
    }
}

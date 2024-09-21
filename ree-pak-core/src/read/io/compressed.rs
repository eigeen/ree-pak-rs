use std::io::{BufRead, Read};

use crate::error::Result;
use crate::pak::CompressionMethod;

/// Read a compressed file.
pub enum CompressedReader<R> {
    Store(R),
    Deflate(flate2::bufread::DeflateDecoder<R>),
    Zstd(zstd::Decoder<'static, R>),
}

impl<R> CompressedReader<R>
where
    R: BufRead,
{
    pub fn new(reader: R, compression: CompressionMethod) -> Result<Self> {
        Ok(match compression {
            CompressionMethod::None => Self::Store(reader),
            CompressionMethod::Deflate => Self::Deflate(flate2::bufread::DeflateDecoder::new(reader)),
            CompressionMethod::Zstd => Self::Zstd(zstd::stream::Decoder::with_buffer(reader)?),
        })
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
    }
}

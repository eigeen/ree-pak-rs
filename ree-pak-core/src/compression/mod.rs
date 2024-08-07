use std::io::Read;

use flate2::read::DeflateDecoder;

type Result<T> = std::result::Result<T, CompressionError>;

#[derive(Debug, thiserror::Error)]
pub enum CompressionError {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
}

pub fn decompress_deflate<R>(reader: &mut R) -> Result<Vec<u8>>
where
    R: Read,
{
    let mut decoder = DeflateDecoder::new(reader);
    let mut output = Vec::new();
    decoder.read_to_end(&mut output)?;

    Ok(output)
}

pub fn decompress_zstd<R>(reader: &mut R) -> Result<Vec<u8>>
where
    R: Read,
{
    let mut output = Vec::new();
    zstd::stream::copy_decode(reader, &mut output)?;

    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use flate2::write::DeflateEncoder;

    use super::*;

    #[test]
    fn test_decompress_zstd() {
        let data = b"Hello, world!";
        let mut compressed = Vec::new();
        zstd::stream::copy_encode(&data[..], &mut compressed, 3).unwrap();
        let decompressed = decompress_zstd(&mut Cursor::new(compressed)).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_decompress_deflate() {
        let data = b"Hello, world!";
        let mut encoder = DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data).unwrap();
        let compressed = encoder.finish().unwrap();

        let mut decompressed = Vec::new();
        let mut decoder = DeflateDecoder::new(&compressed[..]);
        decoder.read_to_end(&mut decompressed).unwrap();
        assert_eq!(decompressed, data);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CompressionMethod {
    #[default]
    None,
    Deflate,
    Zstd,
}

impl From<i64> for CompressionMethod {
    fn from(value: i64) -> Self {
        if value & 0xF == 1 {
            if value >> 16 > 0 {
                CompressionMethod::None
            } else {
                CompressionMethod::Deflate
            }
        } else if value & 0xF == 2 {
            if value >> 16 > 0 {
                CompressionMethod::None
            } else {
                CompressionMethod::Zstd
            }
        } else {
            Self::None
        }
    }
}

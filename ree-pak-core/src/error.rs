pub type Result<T> = std::result::Result<T, PakError>;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum PakError {
    #[error("Upstream IO Error: {0}")]
    IO(#[from] std::io::Error),

    #[error("Invalid Pak file magic: expected {expected:X?}, found {found:X?}")]
    InvalidMagic { expected: [u8; 4], found: [u8; 4] },
    #[error("Unsupported Pak version: {major}.{minor}")]
    UnsupportedVersion { major: u8, minor: u8 },
    #[error("Unsupported algorithm: {0:X}")]
    UnsupportedAlgorithm(u16),
    #[error("Invalid file list: {0}")]
    InvalidFileList(AnyError),

    #[error("Entry index out of bounds")]
    EntryIndexOutOfBounds,
}

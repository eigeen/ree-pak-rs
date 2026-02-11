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
    #[error("Pak contains unsupported feature flags: 0x{0:X}")]
    UnsupportedFeature(crate::pak::FeatureFlags),
    #[error("Invalid file list: {0}")]
    InvalidFileList(AnyError),

    #[error("Entry index out of bounds")]
    EntryIndexOutOfBounds,
    #[error("Invalid UTF-16 sequence")]
    InvalidUtf16,

    #[error("Invalid entry range: offset={offset}, size={size}, file_size={file_size}")]
    InvalidEntryRange { offset: u64, size: u64, file_size: u64 },

    #[error("Failed to build rayon thread pool: {0}")]
    ThreadPoolBuild(String),
}

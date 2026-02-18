mod cipher;
mod entry;
mod flag;
mod header;

use serde::{Deserialize, Serialize};

pub(crate) use cipher::*;
pub use entry::*;
pub use flag::*;
pub use header::*;

/// Pak metadata (header + entry table).
///
/// Note: this struct does **not** include the raw data bytes of the pak file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PakMetadata {
    header: PakHeader,
    entries: Vec<PakEntry>,
}

impl PakMetadata {
    pub fn new(header: PakHeader, entries: Vec<PakEntry>) -> Self {
        PakMetadata { header, entries }
    }

    pub fn header(&self) -> &PakHeader {
        &self.header
    }

    pub fn entries(&self) -> &[PakEntry] {
        &self.entries
    }
}

mod cipher;
mod entry;
mod flag;
mod header;

use serde::{Deserialize, Serialize};

pub(crate) use cipher::*;
pub use entry::*;
pub use flag::*;
pub use header::*;

/// Pak Archive, stores the header and entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PakArchive {
    header: PakHeader,
    entries: Vec<PakEntry>,
}

impl PakArchive {
    pub fn new(header: PakHeader, entries: Vec<PakEntry>) -> Self {
        PakArchive { header, entries }
    }

    pub fn header(&self) -> &PakHeader {
        &self.header
    }

    pub fn entries(&self) -> &[PakEntry] {
        &self.entries
    }
}

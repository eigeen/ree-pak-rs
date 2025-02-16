mod cipher;
mod entry;
mod flag;
mod header;

pub(crate) use cipher::*;
pub use entry::*;
pub use flag::*;
pub use header::*;

/// Pak Archive, stores the header and entries.
#[derive(Clone)]
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

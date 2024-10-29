mod cipher;
mod compression;
mod entry;
mod header;

pub(crate) use cipher::decrypt_data;
pub use compression::CompressionMethod;
pub use entry::PakEntry;
pub use header::PakHeader;

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

    #[inline]
    pub fn header(&self) -> &PakHeader {
        &self.header
    }

    #[inline]
    pub fn entries(&self) -> &[PakEntry] {
        &self.entries
    }
}

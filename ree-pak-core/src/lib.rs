//! RE Engine `.pak` core library.
//!
//! This crate focuses on **reading / unpacking** (and some writing) RE Engine pak files, and exposes a layered API:
//!
//! - **High-level**: [`PakFile`] — the “open pak → read/extract entries” handle. It reads and caches
//!   [`pak::PakMetadata`], and uses [`PakReader::try_clone`] to enable parallel-friendly, on-demand entry reads
//!   (including chunk-table entries).
//! - **Mid-level**: [`read::read_metadata`] + [`read::archive::PakMetadataReader`] — parse metadata first, then use any
//!   `Read + Seek` reader to open an entry (note: this path does not support chunk-index offsets).
//! - **Highest-level unpack**: [`UnpackBuilder`] / [`PakFile::extractor`] — bulk extraction with optional file-name table
//!   and filters, plus parallel execution, progress events, and cancellation.
//!
//! # Quick start: open a pak and read one entry
//!
//! ```rust,no_run
//! use std::{fs::File, io::Read};
//! use ree_pak_core::PakFile;
//!
//! # fn main() -> ree_pak_core::Result<()> {
//! let pak = PakFile::from_file(File::open("re_chunk_000.pak")?)?;
//! let entry = &pak.metadata().entries()[0];
//! let mut r = pak.open_entry(entry)?;
//! let mut bytes = Vec::new();
//! r.read_to_end(&mut bytes)?;
//! # Ok(())
//! # }
//! ```
//!
//! # Extract to the filesystem (with a file-name table)
//!
//! ```rust,no_run
//! use std::{fs::File, sync::Arc};
//! use ree_pak_core::{FileNameTable, PakFile};
//!
//! # fn main() -> ree_pak_core::Result<()> {
//! let pak = PakFile::from_file(File::open("re_chunk_000.pak")?)?;
//! let names = Arc::new(FileNameTable::from_list_file("MHRS_PC_Demo.list")?);
//!
//! let report = pak
//!     .extractor("out")
//!     .file_name_table_arc(names)
//!     .overwrite(true)
//!     .run()?;
//!
//! println!("extracted={}, skipped={}, failed={}", report.extracted, report.skipped, report.failed);
//! # Ok(())
//! # }
//! ```
//!
//! # Module overview
//!
//! - [`pak`]: header/entries/flags data structures (serde-friendly).
//! - [`pakfile`]: high-level handle (recommended read entry point).
//! - [`read`]: lower-level readers (metadata / entry reader / chunk table).
//! - [`extract`]: batch extraction (to FS or via callbacks).
//! - [`write`]: pak writing (currently a minimal, pragmatic writer path).
//! - [`filename`]: file-name table (`hash -> UTF-16 path`) parsing and lookup.

pub mod error;
pub mod extract;
pub mod filename;
pub mod pak;
pub mod pakfile;
pub mod read;
pub mod utf16_hash;
pub mod write;

mod serde_util;
mod spec;

// Commonly-used re-exports for ergonomic crate usage.
pub use error::PakError;
pub use error::Result;
pub use extract::{
    ExtractEvent, ExtractMode, ExtractReport, PakExtractBuilder, PakExtractCallbackBuilder, UnpackBuilder,
};
pub use filename::FileNameTable;
pub use pakfile::{CloneableFile, MmapFile, PakFile, PakReader};
pub use read::PakReadOptions;

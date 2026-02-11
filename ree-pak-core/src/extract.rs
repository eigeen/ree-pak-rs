use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rayon::prelude::*;

use crate::error::{PakError, Result};
use crate::filename::FileNameTable;
use crate::pak::PakEntry;
use crate::pakfile::PakBackend;
use crate::pakfile::PakFile;

type EntryFilter = dyn Fn(&PakEntry, Option<&str>) -> bool + Send + Sync;

#[derive(Debug, Clone)]
pub enum ExtractEvent {
    Start {
        total: usize,
    },
    FileStart {
        hash: u64,
        path: PathBuf,
    },
    FileDone {
        hash: u64,
        path: PathBuf,
        error: Option<String>,
    },
    Finish {
        extracted: usize,
        skipped: usize,
        failed: usize,
    },
    Aborted,
}

pub struct ExtractReport {
    pub extracted: usize,
    pub skipped: usize,
    pub failed: usize,
    pub errors: Vec<(u64, PathBuf, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractMode {
    Parallel,
    Sequential,
}

impl Default for ExtractMode {
    fn default() -> Self {
        Self::Parallel
    }
}

pub struct PakExtractBuilder<'a> {
    pak: &'a PakFile,
    output_dir: PathBuf,
    mode: ExtractMode,
    threads: Option<usize>,
    overwrite: bool,
    skip_unknown: bool,
    continue_on_error: bool,
    file_name_table: Option<Arc<FileNameTable>>,
    filter: Option<Arc<EntryFilter>>,
    on_event: Option<Arc<dyn Fn(ExtractEvent) + Send + Sync>>,
    cancel_flag: Option<Arc<AtomicBool>>,
}

impl<'a> PakExtractBuilder<'a> {
    pub fn new(pak: &'a PakFile, output_dir: impl AsRef<Path>) -> Self {
        Self {
            pak,
            output_dir: output_dir.as_ref().to_path_buf(),
            mode: ExtractMode::default(),
            threads: None,
            overwrite: false,
            skip_unknown: false,
            continue_on_error: false,
            file_name_table: None,
            filter: None,
            on_event: None,
            cancel_flag: None,
        }
    }

    pub fn mode(mut self, mode: ExtractMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn parallel(mut self, enabled: bool) -> Self {
        self.mode = if enabled {
            ExtractMode::Parallel
        } else {
            ExtractMode::Sequential
        };
        self
    }

    pub fn threads(mut self, threads: usize) -> Self {
        self.threads = Some(threads);
        self
    }

    pub fn overwrite(mut self, overwrite: bool) -> Self {
        self.overwrite = overwrite;
        self
    }

    pub fn skip_unknown(mut self, skip_unknown: bool) -> Self {
        self.skip_unknown = skip_unknown;
        self
    }

    pub fn continue_on_error(mut self, continue_on_error: bool) -> Self {
        self.continue_on_error = continue_on_error;
        self
    }

    pub fn file_name_table(mut self, table: FileNameTable) -> Self {
        self.file_name_table = Some(Arc::new(table));
        self
    }

    pub fn file_name_table_arc(mut self, table: Arc<FileNameTable>) -> Self {
        self.file_name_table = Some(table);
        self
    }

    pub fn filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&PakEntry, Option<&str>) -> bool + Send + Sync + 'static,
    {
        self.filter = Some(Arc::new(filter));
        self
    }

    pub fn on_event<F>(mut self, on_event: F) -> Self
    where
        F: Fn(ExtractEvent) + Send + Sync + 'static,
    {
        self.on_event = Some(Arc::new(on_event));
        self
    }

    pub fn cancel_flag(mut self, cancel_flag: Arc<AtomicBool>) -> Self {
        self.cancel_flag = Some(cancel_flag);
        self
    }

    pub fn run(self) -> Result<ExtractReport> {
        if !self.output_dir.exists() {
            std::fs::create_dir_all(&self.output_dir)?;
        }

        let mut tasks: Vec<(PakEntry, PathBuf)> = Vec::new();
        let mut skipped = 0usize;

        for entry in self.pak.archive().entries() {
            let (path_str, rel_path) = match &self.file_name_table {
                Some(table) => match table.get_file_name(entry.hash()) {
                    Some(name) => {
                        let s = name.to_string()?;
                        let rel = PathBuf::from(&s);
                        (Some(s), rel)
                    }
                    None => {
                        if self.skip_unknown {
                            skipped += 1;
                            continue;
                        }
                        (None, PathBuf::from(format!("_Unknown/{:08X}", entry.hash())))
                    }
                },
                None => {
                    if self.skip_unknown {
                        skipped += 1;
                        continue;
                    }
                    (None, PathBuf::from(format!("_Unknown/{:08X}", entry.hash())))
                }
            };

            if let Some(filter) = &self.filter
                && !filter(entry, path_str.as_deref())
            {
                skipped += 1;
                continue;
            }

            tasks.push((entry.clone(), rel_path));
        }

        if let Some(on_event) = &self.on_event {
            on_event(ExtractEvent::Start { total: tasks.len() });
        }

        if let Some(flag) = &self.cancel_flag
            && flag.load(Ordering::Relaxed)
        {
            if let Some(on_event) = &self.on_event {
                on_event(ExtractEvent::Aborted);
            }
            return Ok(ExtractReport {
                extracted: 0,
                skipped,
                failed: 0,
                errors: vec![],
            });
        }

        let errors: Arc<Mutex<Vec<(u64, PathBuf, String)>>> = Arc::new(Mutex::new(vec![]));
        let extracted = Arc::new(AtomicCount::new());

        let work = || -> Result<()> {
            match (self.mode, self.continue_on_error) {
                (ExtractMode::Sequential, _) => {
                    for (entry, rel_path) in &tasks {
                        if self.should_abort() {
                            return Ok(());
                        }
                        if let Some(on_event) = &self.on_event {
                            on_event(ExtractEvent::FileStart {
                                hash: entry.hash(),
                                path: rel_path.clone(),
                            });
                        }
                        let out_path = self.output_dir.join(rel_path);
                        let result = self.extract_one(entry, &out_path);
                        match result {
                            Ok(()) => extracted.inc(),
                            Err(e) => {
                                let msg = e.to_string();
                                errors
                                    .lock()
                                    .unwrap()
                                    .push((entry.hash(), rel_path.clone(), msg.clone()));
                                if let Some(on_event) = &self.on_event {
                                    on_event(ExtractEvent::FileDone {
                                        hash: entry.hash(),
                                        path: rel_path.clone(),
                                        error: Some(msg),
                                    });
                                }
                                if !self.continue_on_error {
                                    return Err(e);
                                }
                                continue;
                            }
                        }
                        if let Some(on_event) = &self.on_event {
                            on_event(ExtractEvent::FileDone {
                                hash: entry.hash(),
                                path: rel_path.clone(),
                                error: None,
                            });
                        }
                    }
                }
                (ExtractMode::Parallel, false) => {
                    tasks.par_iter().try_for_each(|(entry, rel_path)| -> Result<()> {
                        if self.should_abort() {
                            return Ok(());
                        }
                        if let Some(on_event) = &self.on_event {
                            on_event(ExtractEvent::FileStart {
                                hash: entry.hash(),
                                path: rel_path.clone(),
                            });
                        }
                        let out_path = self.output_dir.join(rel_path);
                        let result = self.extract_one(entry, &out_path);
                        if let Some(on_event) = &self.on_event {
                            on_event(ExtractEvent::FileDone {
                                hash: entry.hash(),
                                path: rel_path.clone(),
                                error: result.as_ref().err().map(|e| e.to_string()),
                            });
                        }
                        result?;
                        extracted.inc();
                        Ok(())
                    })?;
                }
                (ExtractMode::Parallel, true) => {
                    tasks.par_iter().for_each(|(entry, rel_path)| {
                        if self.should_abort() {
                            return;
                        }
                        if let Some(on_event) = &self.on_event {
                            on_event(ExtractEvent::FileStart {
                                hash: entry.hash(),
                                path: rel_path.clone(),
                            });
                        }
                        let out_path = self.output_dir.join(rel_path);
                        let result = self.extract_one(entry, &out_path);
                        if let Err(e) = &result {
                            errors
                                .lock()
                                .unwrap()
                                .push((entry.hash(), rel_path.clone(), e.to_string()));
                        } else {
                            extracted.inc();
                        }
                        if let Some(on_event) = &self.on_event {
                            on_event(ExtractEvent::FileDone {
                                hash: entry.hash(),
                                path: rel_path.clone(),
                                error: result.as_ref().err().map(|e| e.to_string()),
                            });
                        }
                    });
                }
            }
            Ok(())
        };

        if self.mode == ExtractMode::Parallel {
            if let Some(n) = self.threads {
                let pool = rayon::ThreadPoolBuilder::new()
                    .num_threads(n)
                    .build()
                    .map_err(|e| PakError::ThreadPoolBuild(e.to_string()))?;
                pool.install(work)?;
            } else {
                work()?;
            }
        } else {
            work()?;
        }

        let extracted = extracted.get();
        let errors_vec = errors.lock().unwrap().clone();
        let failed = errors_vec.len();

        if let Some(on_event) = &self.on_event {
            on_event(ExtractEvent::Finish {
                extracted,
                skipped,
                failed,
            });
        }

        Ok(ExtractReport {
            extracted,
            skipped,
            failed,
            errors: errors_vec,
        })
    }

    fn should_abort(&self) -> bool {
        if let Some(flag) = &self.cancel_flag {
            return flag.load(Ordering::Relaxed);
        }
        false
    }

    fn extract_one(&self, entry: &PakEntry, out_path: &Path) -> Result<()> {
        if let Some(parent) = out_path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)?;
        }

        let mut open_options = OpenOptions::new();
        if self.overwrite {
            open_options.create(true).write(true).truncate(true);
        } else {
            open_options.create_new(true).write(true);
        }
        let mut file = open_options.open(out_path)?;

        let mut entry_reader = self.pak.open_entry(entry)?;
        std::io::copy(&mut entry_reader, &mut file)?;
        file.flush()?;

        if out_path.extension().is_none()
            && let Some(ext) = entry_reader.determine_extension()
        {
            let new_path = out_path.with_extension(ext);
            let _ = std::fs::rename(out_path, new_path);
        }

        Ok(())
    }
}

impl PakFile {
    pub fn extractor(&self, output_dir: impl AsRef<Path>) -> PakExtractBuilder<'_> {
        PakExtractBuilder::new(self, output_dir)
    }
}

/// Highest-level unpack API: open pak + extract with builder configuration.
#[derive(Default)]
pub struct UnpackBuilder {
    input: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    backend: PakBackend,
    mode: ExtractMode,
    threads: Option<usize>,
    overwrite: bool,
    skip_unknown: bool,
    continue_on_error: bool,
    file_name_table: Option<Arc<FileNameTable>>,
    filter: Option<Arc<EntryFilter>>,
    on_event: Option<Arc<dyn Fn(ExtractEvent) + Send + Sync>>,
    cancel_flag: Option<Arc<AtomicBool>>,
}

impl UnpackBuilder {
    pub fn builder() -> Self {
        Self::default()
    }

    pub fn input(mut self, input: impl AsRef<Path>) -> Self {
        self.input = Some(input.as_ref().to_path_buf());
        self
    }

    pub fn output_dir(mut self, output_dir: impl AsRef<Path>) -> Self {
        self.output_dir = Some(output_dir.as_ref().to_path_buf());
        self
    }

    pub fn backend(mut self, backend: PakBackend) -> Self {
        self.backend = backend;
        self
    }

    pub fn mode(mut self, mode: ExtractMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn parallel(mut self, enabled: bool) -> Self {
        self.mode = if enabled {
            ExtractMode::Parallel
        } else {
            ExtractMode::Sequential
        };
        self
    }

    pub fn threads(mut self, threads: usize) -> Self {
        self.threads = Some(threads);
        self
    }

    pub fn overwrite(mut self, overwrite: bool) -> Self {
        self.overwrite = overwrite;
        self
    }

    pub fn skip_unknown(mut self, skip_unknown: bool) -> Self {
        self.skip_unknown = skip_unknown;
        self
    }

    pub fn continue_on_error(mut self, continue_on_error: bool) -> Self {
        self.continue_on_error = continue_on_error;
        self
    }

    pub fn file_name_table(mut self, table: FileNameTable) -> Self {
        self.file_name_table = Some(Arc::new(table));
        self
    }

    pub fn file_name_table_arc(mut self, table: Arc<FileNameTable>) -> Self {
        self.file_name_table = Some(table);
        self
    }

    pub fn filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&PakEntry, Option<&str>) -> bool + Send + Sync + 'static,
    {
        self.filter = Some(Arc::new(filter));
        self
    }

    pub fn on_event<F>(mut self, on_event: F) -> Self
    where
        F: Fn(ExtractEvent) + Send + Sync + 'static,
    {
        self.on_event = Some(Arc::new(on_event));
        self
    }

    pub fn cancel_flag(mut self, cancel_flag: Arc<AtomicBool>) -> Self {
        self.cancel_flag = Some(cancel_flag);
        self
    }

    pub fn run(self) -> Result<ExtractReport> {
        let input = self.input.ok_or_else(|| {
            PakError::IO(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Missing input path",
            ))
        })?;
        let output_dir = self.output_dir.ok_or_else(|| {
            PakError::IO(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Missing output dir",
            ))
        })?;

        let pak = PakFile::builder().backend(self.backend).open(&input)?;

        let mut extractor = pak
            .extractor(&output_dir)
            .mode(self.mode)
            .skip_unknown(self.skip_unknown)
            .overwrite(self.overwrite)
            .continue_on_error(self.continue_on_error);

        if let Some(threads) = self.threads {
            extractor = extractor.threads(threads);
        }
        if let Some(table) = self.file_name_table {
            extractor = extractor.file_name_table_arc(table);
        }
        if let Some(filter) = self.filter {
            extractor = extractor.filter(move |entry, path| filter(entry, path));
        }
        if let Some(on_event) = self.on_event {
            extractor = extractor.on_event(move |event| on_event(event));
        }
        if let Some(cancel_flag) = self.cancel_flag {
            extractor = extractor.cancel_flag(cancel_flag);
        }

        extractor.run()
    }
}

struct AtomicCount(std::sync::atomic::AtomicUsize);

impl AtomicCount {
    fn new() -> Self {
        Self(std::sync::atomic::AtomicUsize::new(0))
    }

    fn inc(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    fn get(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

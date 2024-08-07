use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

use byteorder::{LittleEndian, ReadBytesExt};
use entry::{CompressionMethod, EntryKindA, EntryKindB, EntryTable};
use header::{HeaderError, PackageHeader};

use crate::{compression, filename::FileNameTable};

mod cipher;
mod entry;
mod header;

type Result<T> = std::result::Result<T, PackageError>;

#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("Bad header: {0}")]
    BadHeader(#[from] HeaderError),
    #[error("Entry error: {0}")]
    EntryTable(#[from] entry::EntryError),
    #[error("Compression error: {0}")]
    Compression(#[from] compression::CompressionError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub enum ProgressState {
    Decompressed(usize),
    Wrote(usize),
}

/// A PAK file.
pub struct Package {
    entry_table: EntryTable,
    file_name_table: Option<FileNameTable>,
}

impl Package {
    pub fn from_reader<R: Read>(reader: &mut R) -> Result<Self> {
        // parse header
        let mut header_bytes = [0; 16];
        reader.read_exact(&mut header_bytes)?;
        let header = PackageHeader::from_reader(&header_bytes[..])?;

        let mut entry_table_bytes = vec![0; (header.entry_size() * header.total_files) as usize];
        reader.read_exact(&mut entry_table_bytes)?;
        // decrypt
        if header.feature == 8 {
            let mut raw_key = [0; 128];
            reader.read_exact(&mut raw_key)?;
            entry_table_bytes = cipher::decrypt_data(&entry_table_bytes, &raw_key);
        }
        // parse entry table
        let chunks_iter = entry_table_bytes.chunks_exact(header.entry_size() as usize);
        let entry_table = if header.major_version == 2 && header.minor_version == 0 {
            EntryTable::from_iter::<EntryKindA, _, _>(chunks_iter)?
        } else {
            EntryTable::from_iter::<EntryKindB, _, _>(chunks_iter)?
        };

        Ok(Self {
            entry_table,
            file_name_table: None,
        })
    }

    pub fn set_file_name_table(&mut self, file_name_table: FileNameTable) {
        self.file_name_table = Some(file_name_table);
    }

    pub fn file_count(&self) -> usize {
        self.entry_table.len()
    }

    /// Extracts PAK files to the specified directory.
    pub fn export_files<P, R, F>(
        &self,
        output_dir: P,
        file_stream: &mut R,
        progress_callback: Option<F>,
    ) -> Result<()>
    where
        P: AsRef<Path>,
        R: Read + Seek,
        F: Fn(ProgressState),
    {
        for (i, entry) in self.entry_table.into_iter().enumerate() {
            // read compressed data
            file_stream.seek(SeekFrom::Start(entry.offset as u64))?;
            let decompressed_buffer = match entry.compression_method {
                CompressionMethod::None => {
                    let buffer_size = entry.compressed_size.max(entry.uncompressed_size);
                    let mut buffer = vec![0; buffer_size as usize];
                    file_stream.read_exact(&mut buffer)?;
                    buffer
                }
                CompressionMethod::Deflate => {
                    let mut buffer = vec![0; entry.compressed_size as usize];
                    file_stream.read_exact(&mut buffer)?;
                    compression::decompress_deflate(&mut &buffer[..])?
                }
                CompressionMethod::Zstd => {
                    let mut buffer = vec![0; entry.compressed_size as usize];
                    file_stream.read_exact(&mut buffer)?;
                    compression::decompress_zstd(&mut &buffer[..])?
                }
            };
            if let Some(callback) = &progress_callback {
                callback(ProgressState::Decompressed(i));
            }

            // write decompressed data
            let file_name = self.get_file_name(entry.hash()).unwrap_or_else(|| {
                let hash_name = format!("{:0>16X}", entry.hash());
                let path = Path::new("_Unknown").join(hash_name);
                let path = path.to_string_lossy().to_string();
                // detect file extension
                if let Some(ext) = self.detect_file_ext(&mut &decompressed_buffer[..]) {
                    format!("{}{}", path, ext)
                } else {
                    path
                }
            });
            let output_path = output_dir.as_ref().join(file_name);
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut output_file = std::fs::File::create(&output_path)?;
            output_file.write_all(&decompressed_buffer)?;
            if let Some(callback) = &progress_callback {
                callback(ProgressState::Wrote(i));
            }
        }

        Ok(())
    }

    fn get_file_name(&self, hash: u64) -> Option<String> {
        self.file_name_table
            .as_ref()
            .and_then(|table| table.get_file_name(hash))
            .map(|n| n.get_name())
    }

    fn detect_file_ext<R: Read>(&self, mut buffer: R) -> Option<&'static str> {
        let magic1 = buffer.read_u32::<LittleEndian>().ok()?;
        match magic1 {
            0x1D8 => return Some(".motlist"),
            0x424454 => return Some(".tdb"),
            0x424956 => return Some(".vib"),
            0x444957 => return Some(".wid"),
            0x444F4C => return Some(".lod"),
            0x444252 => return Some(".rbd"),
            0x4C4452 => return Some(".rdl"),
            0x424650 => return Some(".pfb"),
            0x464453 => return Some(".mmtr"),
            0x46444D => return Some(".mdf2"),
            0x4C4F46 => return Some(".fol"),
            0x4E4353 => return Some(".scn"),
            0x4F4C43 => return Some(".clo"),
            0x504D4C => return Some(".lmp"),
            0x535353 => return Some(".sss"),
            0x534549 => return Some(".ies"),
            0x530040 => return Some(".wel"),
            0x584554 => return Some(".tex"),
            0x525355 => return Some(".user"),
            0x5A5352 => return Some(".wcc"),
            0x4C4750 => return Some(".pgl"),
            0x474F50 => return Some(".pog"),
            0x4C4D47 => return Some(".gml"),
            0x4034B50 => return Some(".zip"),
            0x444E5247 => return Some(".grnd"),
            0x20204648 => return Some(".hf"),
            0x0A4C5447 => return Some(".gtl"),
            0x4B424343 => return Some(".ccbk"),
            0x20464843 => return Some(".chf"),
            0x4854444D => return Some(".mdth"),
            0x5443504D => return Some(".mpct"),
            0x594C504D => return Some(".mply"),
            0x50415257 => return Some(".wrap"),
            0x50534C43 => return Some(".clsp"),
            0x4F49434F => return Some(".ocio"),
            0x4F434F43 => return Some(".coco"),
            0x5F525350 => return Some(".psr_bvhl"),
            0x4403FBF5 => return Some(".ncf"),
            0x5DD45FC6 => return Some(".ncf"),
            0x444D5921 => return Some(".ymd"),
            0x52544350 => return Some(".pctr"),
            0x44474C4D => return Some(".mlgd"),
            0x20434452 => return Some(".rdc"),
            0x50464E4E => return Some(".nnfp"),
            0x4D534C43 => return Some(".clsm"),
            0x54414D2E => return Some(".mat"),
            0x54464453 => return Some(".sdft"),
            0x44424453 => return Some(".sdbd"),
            0x52554653 => return Some(".sfur"),
            0x464E4946 => return Some(".finf"),
            0x4D455241 => return Some(".arem"),
            0x21545353 => return Some(".sst"),
            0x204D4252 => return Some(".rbm"),
            0x4D534648 => return Some(".hfsm"),
            0x59444F42 => return Some(".rdd"),
            0x20464544 => return Some(".def"),
            0x4252504E => return Some(".nprb"),
            0x44484B42 => return Some(".bnk"),
            0x75B22630 => return Some(".mov"),
            0x4853454D => return Some(".mesh"),
            0x4B504B41 => return Some(".pck"),
            0x50534552 => return Some(".spmdl"),
            0x54564842 => return Some(".fsmv2"),
            0x4C4F4352 => return Some(".rcol"),
            0x5556532E => return Some(".uvs"),
            0x4C494643 => return Some(".cfil"),
            0x54504E47 => return Some(".gnpt"),
            0x54414D43 => return Some(".cmat"),
            0x44545254 => return Some(".trtd"),
            0x50494C43 => return Some(".clip"),
            0x564D4552 => return Some(".mov"),
            0x414D4941 => return Some(".aimapattr"),
            0x504D4941 => return Some(".aimp"),
            0x72786665 => return Some(".efx"),
            0x736C6375 => return Some(".ucls"),
            0x54435846 => return Some(".fxct"),
            0x58455452 => return Some(".rtex"),
            0x37863546 => return Some(".oft"),
            0x4F464246 => return Some(".oft"),
            0x4C4F434D => return Some(".mcol"),
            0x46454443 => return Some(".cdef"),
            0x504F5350 => return Some(".psop"),
            0x454D414D => return Some(".mame"),
            0x43414D4D => return Some(".mameac"),
            0x544C5346 => return Some(".fslt"),
            0x64637273 => return Some(".srcd"),
            0x68637273 => return Some(".asrc"),
            0x4F525541 => return Some(".auto"),
            0x7261666C => return Some(".lfar"),
            0x52524554 => return Some(".terr"),
            0x736E636A => return Some(".jcns"),
            0x6C626C74 => return Some(".tmlbld"),
            0x54455343 => return Some(".cset"),
            0x726D6565 => return Some(".eemr"),
            0x434C4244 => return Some(".dblc"),
            0x384D5453 => return Some(".stmesh"),
            0x32736674 => return Some(".tmlfsm2"),
            0x45555141 => return Some(".aque"),
            0x46554247 => return Some(".gbuf"),
            0x4F4C4347 => return Some(".gclo"),
            0x44525453 => return Some(".srtd"),
            0x544C4946 => return Some(".filt"),
            _ => {}
        };
        let magic2 = buffer.read_u32::<LittleEndian>().ok()?;
        match magic2 {
            0x766544 => return Some(".dev"),
            0x6B696266 => return Some(".fbik"),
            0x74646566 => return Some(".fedt"),
            0x73627472 => return Some(".rtbs"),
            0x67727472 => return Some(".rtrg"),
            0x67636B69 => return Some(".ikcg"),
            0x45445046 => return Some(".fpde"),
            0x64776863 => return Some(".chwd"),
            0x6E616863 => return Some(".chain"),
            0x6E6C6B73 => return Some(".fbxskel"),
            0x47534D47 => return Some(".msg"),
            0x52495547 => return Some(".gui"),
            0x47464347 => return Some(".gcfg"),
            0x72617675 => return Some(".uvar"),
            0x544E4649 => return Some(".ifnt"),
            0x20746F6D => return Some(".mot"),
            0x70797466 => return Some(".mov"),
            0x6D61636D => return Some(".mcam"),
            0x6572746D => return Some(".mtre"),
            0x6D73666D => return Some(".mfsm"),
            0x74736C6D => return Some(".motlist"),
            0x6B6E626D => return Some(".motbank"),
            0x3273666D => return Some(".motfsm2"),
            0x74736C63 => return Some(".mcamlist"),
            0x70616D6A => return Some(".jmap"),
            0x736E636A => return Some(".jcns"),
            0x4E414554 => return Some(".tean"),
            0x61646B69 => return Some(".ikda"),
            0x736C6B69 => return Some(".ikls"),
            0x72746B69 => return Some(".iktr"),
            0x326C6B69 => return Some(".ikl2"),
            0x72686366 => return Some(".fchr"),
            0x544C5346 => return Some(".fslt"),
            0x6B6E6263 => return Some(".cbnk"),
            0x30474154 => return Some(".havokcl"),
            0x52504347 => return Some(".gcpr"),
            0x74646366 => return Some(".fcmndatals"),
            0x67646C6A => return Some(".jointlodgroup"),
            0x444E5347 => return Some(".gsnd"),
            0x59545347 => return Some(".gsty"),
            0x3267656C => return Some(".leg2"),
            _ => {}
        };

        None
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_package() {
        let file = std::fs::File::open("../test_files/re_chunk_000.pak").unwrap();
        let mut reader = std::io::BufReader::new(file);
        let _package = Package::from_reader(&mut reader).unwrap();
    }

    #[test]
    fn test_export_files() {
        let file = std::fs::File::open("../test_files/re_chunk_000.pak").unwrap();
        let mut reader = std::io::BufReader::new(file);
        let mut package = Package::from_reader(&mut reader).unwrap();

        let file_name_list = Path::new("../assets/filelist/MHRS_PC_Demo.list");
        let file_name_table = FileNameTable::from_list_file(file_name_list).unwrap();
        package.set_file_name_table(file_name_table);
        let output_dir = Path::new("test_files/output");
        package
            .export_files(output_dir, &mut reader, None::<fn(ProgressState)>)
            .unwrap();
    }
}

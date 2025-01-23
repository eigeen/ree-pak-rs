use std::io::Read;

/// Reads the first 8 bytes to determine the file extension.
pub struct ExtensionReader<R> {
    reader: R,
    magic_bytes: [u8; 8],
    magic_read_length: usize,
}

impl<R> Read for ExtensionReader<R>
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.magic_read_length < 8 {
            let bytes_to_read = 8 - self.magic_read_length;
            let bytes_read = self
                .reader
                .read(&mut self.magic_bytes[self.magic_read_length..bytes_to_read])?;
            self.magic_read_length += bytes_read;

            let bytes_to_copy = self.magic_read_length.min(buf.len());
            buf[..bytes_to_copy].copy_from_slice(&self.magic_bytes[..bytes_to_copy]);

            if bytes_to_copy == 8 {
                let remaining = &mut buf[bytes_to_copy..];
                let additional_read = self.reader.read(remaining)?;
                return Ok(bytes_to_copy + additional_read);
            }

            return Ok(bytes_to_copy);
        }

        self.reader.read(buf)
    }
}

impl<R> ExtensionReader<R>
where
    R: Read,
{
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            magic_bytes: [0; 8],
            magic_read_length: 0,
        }
    }

    pub fn magic_bytes(&self) -> &[u8; 8] {
        &self.magic_bytes
    }

    pub fn magic_u64(&self) -> u64 {
        u64::from_le_bytes(self.magic_bytes)
    }

    pub fn magic_lower(&self) -> u32 {
        u32::from_le_bytes(self.magic_bytes[0..4].try_into().unwrap())
    }

    pub fn magic_upper(&self) -> u32 {
        u32::from_le_bytes(self.magic_bytes[4..8].try_into().unwrap())
    }

    pub fn determine_extension(&self) -> Option<&'static str> {
        if self.magic_read_length < 8 {
            return None;
        }

        match self.magic_lower() {
            0x1D8 => return Some("motlist"),
            0x424454 => return Some("tdb"),
            0x424956 => return Some("vib"),
            0x444957 => return Some("wid"),
            0x444F4C => return Some("lod"),
            0x444252 => return Some("rbd"),
            0x4C4452 => return Some("rdl"),
            0x424650 => return Some("pfb"),
            0x464453 => return Some("mmtr"),
            0x46444D => return Some("mdf2"),
            0x4C4F46 => return Some("fol"),
            0x4E4353 => return Some("scn"),
            0x4F4C43 => return Some("clo"),
            0x504D4C => return Some("lmp"),
            0x535353 => return Some("sss"),
            0x534549 => return Some("ies"),
            0x530040 => return Some("wel"),
            0x584554 => return Some("tex"),
            0x525355 => return Some("user"),
            0x5A5352 => return Some("wcc"),
            0x4C4750 => return Some("pgl"),
            0x474F50 => return Some("pog"),
            0x4C4D47 => return Some("gml"),
            0x4034B50 => return Some("zip"),
            0x444E5247 => return Some("grnd"),
            0x20204648 => return Some("hf"),
            0x0A4C5447 => return Some("gtl"),
            0x4B424343 => return Some("ccbk"),
            0x20464843 => return Some("chf"),
            0x4854444D => return Some("mdth"),
            0x5443504D => return Some("mpct"),
            0x594C504D => return Some("mply"),
            0x50415257 => return Some("wrap"),
            0x50534C43 => return Some("clsp"),
            0x4F49434F => return Some("ocio"),
            0x4F434F43 => return Some("coco"),
            0x5F525350 => return Some("psr_bvhl"),
            0x4403FBF5 => return Some("ncf"),
            0x5DD45FC6 => return Some("ncf"),
            0x444D5921 => return Some("ymd"),
            0x52544350 => return Some("pctr"),
            0x44474C4D => return Some("mlgd"),
            0x20434452 => return Some("rdc"),
            0x50464E4E => return Some("nnfp"),
            0x4D534C43 => return Some("clsm"),
            0x54414D2E => return Some("mat"),
            0x54464453 => return Some("sdft"),
            0x44424453 => return Some("sdbd"),
            0x52554653 => return Some("sfur"),
            0x464E4946 => return Some("finf"),
            0x4D455241 => return Some("arem"),
            0x21545353 => return Some("sst"),
            0x204D4252 => return Some("rbm"),
            0x4D534648 => return Some("hfsm"),
            0x59444F42 => return Some("rdd"),
            0x20464544 => return Some("def"),
            0x4252504E => return Some("nprb"),
            0x44484B42 => return Some("bnk"),
            0x75B22630 => return Some("mov"),
            0x4853454D => return Some("mesh"),
            0x4B504B41 => return Some("pck"),
            0x50534552 => return Some("spmdl"),
            0x54564842 => return Some("fsmv2"),
            0x4C4F4352 => return Some("rcol"),
            0x5556532E => return Some("uvs"),
            0x4C494643 => return Some("cfil"),
            0x54504E47 => return Some("gnpt"),
            0x54414D43 => return Some("cmat"),
            0x44545254 => return Some("trtd"),
            0x50494C43 => return Some("clip"),
            0x564D4552 => return Some("mov"),
            0x414D4941 => return Some("aimapattr"),
            0x504D4941 => return Some("aimp"),
            0x72786665 => return Some("efx"),
            0x736C6375 => return Some("ucls"),
            0x54435846 => return Some("fxct"),
            0x58455452 => return Some("rtex"),
            0x37863546 => return Some("oft"),
            0x4F464246 => return Some("oft"),
            0x4C4F434D => return Some("mcol"),
            0x46454443 => return Some("cdef"),
            0x504F5350 => return Some("psop"),
            0x454D414D => return Some("mame"),
            0x43414D4D => return Some("mameac"),
            0x544C5346 => return Some("fslt"),
            0x64637273 => return Some("srcd"),
            0x68637273 => return Some("asrc"),
            0x4F525541 => return Some("auto"),
            0x7261666C => return Some("lfar"),
            0x52524554 => return Some("terr"),
            0x736E636A => return Some("jcns"),
            0x6C626C74 => return Some("tmlbld"),
            0x54455343 => return Some("cset"),
            0x726D6565 => return Some("eemr"),
            0x434C4244 => return Some("dblc"),
            0x384D5453 => return Some("stmesh"),
            0x32736674 => return Some("tmlfsm2"),
            0x45555141 => return Some("aque"),
            0x46554247 => return Some("gbuf"),
            0x4F4C4347 => return Some("gclo"),
            0x44525453 => return Some("srtd"),
            0x544C4946 => return Some("filt"),
            _ => {}
        };
        match self.magic_upper() {
            0x766544 => return Some("dev"),
            0x6B696266 => return Some("fbik"),
            0x74646566 => return Some("fedt"),
            0x73627472 => return Some("rtbs"),
            0x67727472 => return Some("rtrg"),
            0x67636B69 => return Some("ikcg"),
            0x45445046 => return Some("fpde"),
            0x64776863 => return Some("chwd"),
            0x6E616863 => return Some("chain"),
            0x6E6C6B73 => return Some("fbxskel"),
            0x47534D47 => return Some("msg"),
            0x52495547 => return Some("gui"),
            0x47464347 => return Some("gcfg"),
            0x72617675 => return Some("uvar"),
            0x544E4649 => return Some("ifnt"),
            0x20746F6D => return Some("mot"),
            0x70797466 => return Some("mov"),
            0x6D61636D => return Some("mcam"),
            0x6572746D => return Some("mtre"),
            0x6D73666D => return Some("mfsm"),
            0x74736C6D => return Some("motlist"),
            0x6B6E626D => return Some("motbank"),
            0x3273666D => return Some("motfsm2"),
            0x74736C63 => return Some("mcamlist"),
            0x70616D6A => return Some("jmap"),
            0x736E636A => return Some("jcns"),
            0x4E414554 => return Some("tean"),
            0x61646B69 => return Some("ikda"),
            0x736C6B69 => return Some("ikls"),
            0x72746B69 => return Some("iktr"),
            0x326C6B69 => return Some("ikl2"),
            0x72686366 => return Some("fchr"),
            0x544C5346 => return Some("fslt"),
            0x6B6E6263 => return Some("cbnk"),
            0x30474154 => return Some("havokcl"),
            0x52504347 => return Some("gcpr"),
            0x74646366 => return Some("fcmndatals"),
            0x67646C6A => return Some("jointlodgroup"),
            0x444E5347 => return Some("gsnd"),
            0x59545347 => return Some("gsty"),
            0x3267656C => return Some("leg2"),
            _ => {}
        };

        None
    }
}

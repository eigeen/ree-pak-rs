use std::io::Read;

#[derive(Debug, Clone)]
#[repr(C)]
pub struct Header {
    pub magic: [u8; 4],
    pub major_version: u8,
    pub minor_version: u8,
    pub feature: u16,
    pub total_files: u32,
    pub hash: u32,
}

impl Header {
    const SIZE: usize = std::mem::size_of::<Self>();

    pub fn from_reader<R>(reader: &mut R) -> Self
    where
        R: Read,
    {
        let mut buf = [0u8; Self::SIZE];
        reader.read_exact(&mut buf).unwrap();
        unsafe { std::mem::transmute(buf) }
    }
}

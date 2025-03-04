use std::io::{Seek, Write};

pub struct PakWriter<W: Write + Seek> {
    pub(crate) inner: W,
}

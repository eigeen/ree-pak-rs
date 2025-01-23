use std::io::{BufRead, Read};

use crate::pak::{self, EncryptionType};

pub struct EncryptedReader<R> {
    reader: R,
    encryption: EncryptionType,
    buffer: Vec<u8>,
    has_decrypted: bool,
}

impl<R> EncryptedReader<R>
where
    R: BufRead,
{
    pub fn new(reader: R, encryption: EncryptionType) -> Self {
        Self {
            reader,
            encryption,
            buffer: Vec::new(),
            has_decrypted: false,
        }
    }
}

impl<R> EncryptedReader<R> {
    pub fn is_encrypted(&self) -> bool {
        self.encryption != EncryptionType::None && self.encryption != EncryptionType::TypeInvalid
    }
}

impl<R> EncryptedReader<R>
where
    R: Read,
{
    pub fn decrypt_fill_buf(&mut self) -> std::io::Result<()> {
        self.has_decrypted = true;
        let decrypted_data = pak::decrypt_resource_data(&mut self.reader)?;
        self.buffer.extend_from_slice(&decrypted_data);
        Ok(())
    }
}

impl<R> Read for EncryptedReader<R>
where
    R: BufRead,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if !self.is_encrypted() {
            return self.reader.read(buf);
        }

        if !self.has_decrypted {
            self.decrypt_fill_buf()?;
        }

        let len = std::cmp::min(buf.len(), self.buffer.len());
        if len == 0 {
            println!("encrypted reader: buffer empty");
            return Ok(0);
            // return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Unexpected EOF"));
        }

        buf[..len].copy_from_slice(&self.buffer[..len]);
        self.buffer = self.buffer.split_off(len);

        Ok(len)
    }
}

impl<R> BufRead for EncryptedReader<R>
where
    R: BufRead,
{
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if !self.is_encrypted() {
            return self.reader.fill_buf();
        }

        if !self.has_decrypted {
            self.decrypt_fill_buf()?;
        }

        Ok(&self.buffer)
    }

    fn consume(&mut self, amt: usize) {
        if !self.is_encrypted() {
            self.reader.consume(amt);
            return;
        }

        self.buffer = self.buffer.split_off(amt);
    }
}

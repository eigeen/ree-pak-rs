use std::{io::Read, sync::LazyLock};

use byteorder::{ReadBytesExt, LE};
use num::BigUint;

const MODULUS: [u8; 33] = [
    0x13, 0xD7, 0x9C, 0x89, 0x88, 0x91, 0x48, 0x10, 0xD7, 0xAA, 0x78, 0xAE, 0xF8, 0x59, 0xDF, 0x7D, 0x3C, 0x43, 0xA0,
    0xD0, 0xBB, 0x36, 0x77, 0xB5, 0xF0, 0x5C, 0x02, 0xAF, 0x65, 0xD8, 0x77, 0x03, 0x00,
];
const EXPONENT: [u8; 33] = [
    0xC0, 0xC2, 0x77, 0x1F, 0x5B, 0x34, 0x6A, 0x01, 0xC7, 0xD4, 0xD7, 0x85, 0x2E, 0x42, 0x2B, 0x3B, 0x16, 0x3A, 0x17,
    0x13, 0x16, 0xEA, 0x83, 0x30, 0x30, 0xDF, 0x3F, 0xF4, 0x25, 0x93, 0x20, 0x01, 0x00,
];

static MODULUS_INT: LazyLock<BigUint> = LazyLock::new(|| BigUint::from_bytes_le(&MODULUS));
static EXPONENT_INT: LazyLock<BigUint> = LazyLock::new(|| BigUint::from_bytes_le(&EXPONENT));

pub fn decrypt_resource_data<R>(reader: &mut R) -> std::io::Result<Vec<u8>>
where
    R: Read,
{
    let decrypted_size = reader.read_u64::<LE>()?;
    let mut decrypted_data = Vec::with_capacity((decrypted_size + 1) as usize);

    loop {
        let mut chunk_buf = [0u8; 128];
        if let Err(e) = reader.read_exact(&mut chunk_buf) {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                break;
            } else {
                return Err(e);
            }
        };

        let key = BigUint::from_bytes_le(&chunk_buf[0..64]);
        let data = BigUint::from_bytes_le(&chunk_buf[64..128]);

        let r#mod = key.modpow(&EXPONENT_INT, &MODULUS_INT);
        let result = data / r#mod;

        let digits = result.to_u64_digits().first().cloned();
        if let Some(digits) = digits {
            decrypted_data.extend_from_slice(&digits.to_le_bytes());
        }
    }

    // remove padding zeros
    while decrypted_data.last() == Some(&0) {
        decrypted_data.pop();
    }

    Ok(decrypted_data)
}

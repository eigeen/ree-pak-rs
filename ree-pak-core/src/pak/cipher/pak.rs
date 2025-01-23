use std::sync::LazyLock;

use num::BigUint;

const MODULUS: [u8; 129] = [
    0x7D, 0x0B, 0xF8, 0xC1, 0x7C, 0x23, 0xFD, 0x3B, 0xD4, 0x75, 0x16, 0xD2, 0x33, 0x21, 0xD8, 0x10, 0x71, 0xF9, 0x7C,
    0xD1, 0x34, 0x93, 0xBA, 0x77, 0x26, 0xFC, 0xAB, 0x2C, 0xEE, 0xDA, 0xD9, 0x1C, 0x89, 0xE7, 0x29, 0x7B, 0xDD, 0x8A,
    0xAE, 0x50, 0x39, 0xB6, 0x01, 0x6D, 0x21, 0x89, 0x5D, 0xA5, 0xA1, 0x3E, 0xA2, 0xC0, 0x8C, 0x93, 0x13, 0x36, 0x65,
    0xEB, 0xE8, 0xDF, 0x06, 0x17, 0x67, 0x96, 0x06, 0x2B, 0xAC, 0x23, 0xED, 0x8C, 0xB7, 0x8B, 0x90, 0xAD, 0xEA, 0x71,
    0xC4, 0x40, 0x44, 0x9D, 0x1C, 0x7B, 0xBA, 0xC4, 0xB6, 0x2D, 0xD6, 0xD2, 0x4B, 0x62, 0xD6, 0x26, 0xFC, 0x74, 0x20,
    0x07, 0xEC, 0xE3, 0x59, 0x9A, 0xE6, 0xAF, 0xB9, 0xA8, 0x35, 0x8B, 0xE0, 0xE8, 0xD3, 0xCD, 0x45, 0x65, 0xB0, 0x91,
    0xC4, 0x95, 0x1B, 0xF3, 0x23, 0x1E, 0xC6, 0x71, 0xCF, 0x3E, 0x35, 0x2D, 0x6B, 0xE3, 0x00,
];
const EXPONENT: [u8; 4] = [0x01, 0x00, 0x01, 0x00];

static MODULUS_INT: LazyLock<BigUint> = LazyLock::new(|| BigUint::from_bytes_le(&MODULUS));
static EXPONENT_INT: LazyLock<BigUint> = LazyLock::new(|| BigUint::from_bytes_le(&EXPONENT));

pub fn decrypt_pak_data(data: &[u8], enc_key: &[u8]) -> Vec<u8> {
    let key = decrypt_key(enc_key);
    let mut result = vec![0; data.len()];
    for i in 0..data.len() {
        result[i] = data[i] ^ (i + key[i % 32] as usize * key[i % 29] as usize) as u8;
    }

    result
}

fn decrypt_key(enc_key: &[u8]) -> Vec<u8> {
    let enc_key_int = BigUint::from_bytes_le(&resize_key(enc_key));
    let result_int = enc_key_int.modpow(&EXPONENT_INT, &MODULUS_INT);

    result_int.to_bytes_le()
}

fn resize_key(key: &[u8]) -> Vec<u8> {
    let mut resized_key = key.to_vec();
    resized_key.resize(129, 0);

    resized_key
}

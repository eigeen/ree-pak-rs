use std::io::Read;

pub fn murmur3_hash<R: Read>(mut reader: R) -> Result<u32, std::io::Error> {
    murmur3::murmur3_32(&mut reader, 0xFFFFFFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_string(input: &str) -> u32 {
        let utf16 = input
            .encode_utf16()
            .flat_map(|x| x.to_le_bytes())
            .collect::<Vec<u8>>();
        murmur3_hash(&mut &utf16[..]).unwrap()
    }

    #[test]
    fn test_hash_string() {
        let hash = hash_string("Hello, world!");
        eprintln!("Hash: {:X}", hash);
    }

    #[test]
    fn test_hash_string2() {
        let hash = hash_string("natives/stm/quest/supplydata/supplydata.user.2");
        assert_eq!(hash, 0xD80FAFD3);
    }
}

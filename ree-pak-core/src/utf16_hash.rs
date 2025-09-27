//! UTF-16字符串哈希实现（ASCII优化版本）
//!
//! # ASCII优化策略和影响
//!
//! 为了获得极致性能并避免unsafe操作，本实现采用ASCII-only大小写转换：
//! - 'A'-'Z' (65-90) ↔ 'a'-'z' (97-122)
//! - 其他所有字符（包括Unicode字符）原样输出
//!
//! 这个设计决策基于 RE Engine Pak 主要用于游戏资产路径的事实，几乎所有的路径都是ASCII字符。
//! 对于包含Latin扩展字符的文件名，会产生与标准Unicode大小写转换不同的哈希值，
//! 但这在实际游戏资产中极少出现。

use std::io::Read;

use crate::error::PakError;

pub trait Utf16HashExt {
    fn hash_lower_case(&self) -> u32;
    fn hash_upper_case(&self) -> u32;

    fn hash_mixed(&self) -> u64 {
        let upper = self.hash_upper_case() as u64;
        let lower = self.hash_lower_case() as u64;

        (upper << 32) | lower
    }
}

impl Utf16HashExt for &str {
    fn hash_lower_case(&self) -> u32 {
        let utf16 = Utf16LeString::new_from_str(self);
        utf16.hash_lower_case()
    }

    fn hash_upper_case(&self) -> u32 {
        let utf16 = Utf16LeString::new_from_str(self);
        utf16.hash_upper_case()
    }

    fn hash_mixed(&self) -> u64 {
        let utf16 = Utf16LeString::new_from_str(self);
        utf16.hash_mixed()
    }
}

impl Utf16HashExt for String {
    fn hash_lower_case(&self) -> u32 {
        let utf16 = Utf16LeString::new_from_str(self);
        utf16.hash_lower_case()
    }

    fn hash_upper_case(&self) -> u32 {
        let utf16 = Utf16LeString::new_from_str(self);
        utf16.hash_upper_case()
    }

    fn hash_mixed(&self) -> u64 {
        let utf16 = Utf16LeString::new_from_str(self);
        utf16.hash_mixed()
    }
}

impl Utf16HashExt for u64 {
    fn hash_lower_case(&self) -> u32 {
        (*self & 0xFFFFFFFF) as u32
    }

    fn hash_upper_case(&self) -> u32 {
        (*self >> 32) as u32
    }
}

pub fn murmur3_hash<R: std::io::Read>(mut reader: R) -> std::io::Result<u32> {
    murmur3::murmur3_32(&mut reader, 0xFFFFFFFF)
}

/// UTF-16字符串
///
/// # Hash计算
///
/// 为了获得极致性能并避免unsafe操作，本实现采用ASCII-only大小写转换：
/// - 'A'-'Z' (65-90) ↔ 'a'-'z' (97-122)
/// - 其他所有字符（包括Unicode字符）原样输出
///
/// 这个设计决策基于 RE Engine Pak 主要用于游戏资产路径的事实，几乎所有的路径都是ASCII字符。
/// 对于包含Latin扩展字符的文件名，会产生与标准Unicode大小写转换不同的哈希值，
/// 但这在实际游戏资产中极少出现。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utf16LeString(Vec<u16>);

/// UTF-16大小写转换Reader
///
/// ASCII优化版本
struct Utf16CaseReader<'a> {
    data: &'a [u16],
    position: usize,
    uppercase: bool,
    pending_high_byte: Option<u8>,
}

impl Utf16LeString {
    pub fn new_from_str(s: &str) -> Self {
        let utf16_units: Vec<u16> = s.encode_utf16().collect();
        Self(utf16_units)
    }

    /// 获取UTF-16单元数据
    pub fn as_utf16(&self) -> &[u16] {
        &self.0
    }

    /// 获取UTF-16字节数据（小端序）
    pub fn as_bytes(&self) -> Vec<u8> {
        self.0.iter().flat_map(|&u| u.to_le_bytes()).collect()
    }

    /// 转换回字符串
    pub fn to_string(&self) -> Result<String, PakError> {
        String::from_utf16(&self.0).map_err(|_| PakError::InvalidUtf16)
    }

    /// 获取UTF-16单元长度
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Utf16HashExt for Utf16LeString {
    fn hash_lower_case(&self) -> u32 {
        let mut reader = Utf16CaseReader::new_lowercase(&self.0);
        murmur3_hash(&mut reader).unwrap()
    }

    fn hash_upper_case(&self) -> u32 {
        let mut reader = Utf16CaseReader::new_uppercase(&self.0);
        murmur3_hash(&mut reader).unwrap()
    }
}

impl From<&str> for Utf16LeString {
    fn from(s: &str) -> Self {
        Self::new_from_str(s)
    }
}

impl From<String> for Utf16LeString {
    fn from(s: String) -> Self {
        Self::new_from_str(&s)
    }
}

impl AsRef<[u16]> for Utf16LeString {
    fn as_ref(&self) -> &[u16] {
        &self.0
    }
}

impl<'a> Utf16CaseReader<'a> {
    pub fn new_uppercase(data: &'a [u16]) -> Self {
        Self {
            data,
            position: 0,
            uppercase: true,
            pending_high_byte: None,
        }
    }

    pub fn new_lowercase(data: &'a [u16]) -> Self {
        Self {
            data,
            position: 0,
            uppercase: false,
            pending_high_byte: None,
        }
    }
}

impl<'a> Read for Utf16CaseReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut bytes_read = 0;

        // 先处理上次剩下的高字节
        if let Some(high_byte) = self.pending_high_byte.take() {
            if bytes_read < buf.len() {
                buf[bytes_read] = high_byte;
                bytes_read += 1;
            } else {
                self.pending_high_byte = Some(high_byte);
                return Ok(0);
            }
        }

        // 处理UTF-16数据，每次读取一个UTF-16单元（2字节）
        while self.position < self.data.len() {
            let utf16_unit = self.data[self.position];
            self.position += 1;

            // 只处理ASCII字符的大小写转换
            let converted_unit = if utf16_unit <= 127 {
                if self.uppercase {
                    // 'a'-'z' (97-122) -> 'A'-'Z' (65-90)
                    if (97..=122).contains(&utf16_unit) {
                        utf16_unit - 32
                    } else {
                        utf16_unit
                    }
                } else {
                    // 'A'-'Z' (65-90) -> 'a'-'z' (97-122)
                    if (65..=90).contains(&utf16_unit) {
                        utf16_unit + 32
                    } else {
                        utf16_unit
                    }
                }
            } else {
                // 非ASCII字符
                utf16_unit
            };

            let bytes = converted_unit.to_le_bytes();

            // 输出低字节
            if bytes_read < buf.len() {
                buf[bytes_read] = bytes[0];
                bytes_read += 1;
            } else {
                // 缓冲区满，回退position，等下次调用
                self.position -= 1;
                break;
            }

            // 输出高字节
            if bytes_read < buf.len() {
                buf[bytes_read] = bytes[1];
                bytes_read += 1;
            } else {
                // 缓冲区只能容纳低字节，保存高字节到下次
                self.pending_high_byte = Some(bytes[1]);
                break;
            }
        }

        Ok(bytes_read)
    }
}

#[cfg(feature = "legacy-utf16-hash")]
pub mod legacy {
    /// Legacy UTF-16 hash implements.
    ///
    /// Now it's for testing, benchmark and backward compatibility only.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct FileNameFull {
        name: String,
    }

    impl FileNameFull {
        pub fn new(s: &str) -> Self {
            Self { name: s.to_string() }
        }
    }

    impl crate::utf16_hash::Utf16HashExt for FileNameFull {
        fn hash_lower_case(&self) -> u32 {
            let bytes: Vec<u8> = self
                .name
                .to_lowercase()
                .encode_utf16()
                .flat_map(|c| c.to_le_bytes())
                .collect();

            crate::utf16_hash::murmur3_hash(&bytes[..]).unwrap()
        }

        fn hash_upper_case(&self) -> u32 {
            let bytes: Vec<u8> = self
                .name
                .to_uppercase()
                .encode_utf16()
                .flat_map(|c| c.to_le_bytes())
                .collect();

            crate::utf16_hash::murmur3_hash(&bytes[..]).unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf16_case_reader() {
        let test_string = "Hello.TXT";
        let utf16_str = Utf16LeString::new_from_str(test_string);

        // 测试大写Reader
        let mut upper_reader = Utf16CaseReader::new_uppercase(utf16_str.as_utf16());
        let mut upper_data = Vec::new();
        upper_reader.read_to_end(&mut upper_data).unwrap();

        // 测试小写Reader
        let mut lower_reader = Utf16CaseReader::new_lowercase(utf16_str.as_utf16());
        let mut lower_data = Vec::new();
        lower_reader.read_to_end(&mut lower_data).unwrap();

        assert!(!upper_data.is_empty());
        assert!(!lower_data.is_empty());

        assert_ne!(upper_data, lower_data);
    }

    #[test]
    fn test_utf16_le_string_basic() {
        let test_string = "Hello.TXT";
        let utf16_str = Utf16LeString::new_from_str(test_string);

        assert!(!utf16_str.is_empty());
        assert!(!utf16_str.is_empty());

        // 验证UTF-16转换的正确性
        let expected_utf16: Vec<u16> = test_string.encode_utf16().collect();
        assert_eq!(utf16_str.as_utf16(), &expected_utf16);

        // 验证可以转换回字符串
        assert_eq!(utf16_str.to_string().unwrap(), test_string);
    }

    #[test]
    fn test_utf16_string_mixed_hash() {
        let test_string = "test.file";
        let utf16_str = Utf16LeString::new_from_str(test_string);

        // 测试混合哈希计算
        let mixed_hash = utf16_str.hash_mixed();
        let lower_hash = utf16_str.hash_lower_case();
        let upper_hash = utf16_str.hash_upper_case();

        // 验证混合哈希的正确性
        let expected_mixed = ((upper_hash as u64) << 32) | (lower_hash as u64);
        assert_eq!(mixed_hash, expected_mixed);
    }

    #[test]
    fn test_compatibility_with_known_values() {
        // 使用已知的测试用例验证
        let filename = "natives/stm/camera/collisionfilter/defaultcamera.cfil.7";

        let utf16_str = Utf16LeString::new_from_str(filename);
        assert_eq!(utf16_str.hash_lower_case(), 0x65B486A1);
        assert_eq!(utf16_str.hash_upper_case(), 0x958EDD0C);
        assert_eq!(utf16_str.hash_mixed(), 0x958EDD0C65B486A1);

        // 测试字符串切片实现
        let str_slice: &str = filename;
        assert_eq!(str_slice.hash_lower_case(), 0x65B486A1);
        assert_eq!(str_slice.hash_upper_case(), 0x958EDD0C);
        assert_eq!(str_slice.hash_mixed(), 0x958EDD0C65B486A1);
    }

    #[cfg(feature = "legacy-utf16-hash")]
    #[test]
    fn test_compatibility() {
        let test_cases = vec![
            "test.txt",
            "UPPERCASE.FILE",
            "MiXeD_CaSe.dat",
            "natives/stm/camera/collisionfilter/defaultcamera.cfil.7",
        ];

        for test_str in test_cases {
            // 旧版实现
            let original = FileNameFull::new(test_str);

            // 新优化实现
            let utf16_str = Utf16LeString::new_from_str(test_str);
            let str_impl: &str = test_str;
            let string_impl = test_str.to_string();

            // 确保所有实现产生相同的哈希值
            assert_eq!(utf16_str.hash_lower_case(), original.hash_lower_case());
            assert_eq!(utf16_str.hash_upper_case(), original.hash_upper_case());
            assert_eq!(utf16_str.hash_mixed(), original.hash_mixed());
            assert_eq!(str_impl.hash_mixed(), original.hash_mixed());
            assert_eq!(string_impl.hash_mixed(), original.hash_mixed());
        }
    }
}

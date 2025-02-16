use serde::Serializer;

pub fn serialize_u32_hex<S>(value: &u32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format!("{:08x}", value))
}

pub fn serialize_u64_hex<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format!("{:16x}", value))
}

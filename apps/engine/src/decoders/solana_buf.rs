use crate::error::{EngineError, Result};

pub fn decode_ix_data(data: &str) -> Result<Vec<u8>> {
    bs58::decode(data)
        .into_vec()
        .map_err(|e| EngineError::Malformed(format!("ix data base58: {e}")))
}

pub fn read_string(buf: &[u8], i: &mut usize) -> Result<String> {
    if *i + 4 > buf.len() {
        return Err(EngineError::Malformed("string len".into()));
    }
    let n = u32::from_le_bytes(buf[*i..*i + 4].try_into().unwrap()) as usize;
    *i += 4;
    if *i + n > buf.len() {
        return Err(EngineError::Malformed("string body".into()));
    }
    let s = std::str::from_utf8(&buf[*i..*i + n])
        .map_err(|e| EngineError::Malformed(format!("utf8: {e}")))?
        .to_string();
    *i += n;
    Ok(s)
}

pub fn read_pubkey(buf: &[u8], i: &mut usize) -> Result<String> {
    if *i + 32 > buf.len() {
        return Err(EngineError::Malformed("pubkey".into()));
    }
    let pk = bs58::encode(&buf[*i..*i + 32]).into_string();
    *i += 32;
    Ok(pk)
}

pub fn read_u64(buf: &[u8], i: &mut usize) -> Result<u64> {
    if *i + 8 > buf.len() {
        return Err(EngineError::Malformed("u64".into()));
    }
    let v = u64::from_le_bytes(buf[*i..*i + 8].try_into().unwrap());
    *i += 8;
    Ok(v)
}

pub fn read_i64(buf: &[u8], i: &mut usize) -> Result<i64> {
    if *i + 8 > buf.len() {
        return Err(EngineError::Malformed("i64".into()));
    }
    let v = i64::from_le_bytes(buf[*i..*i + 8].try_into().unwrap());
    *i += 8;
    Ok(v)
}

pub fn read_bool(buf: &[u8], i: &mut usize) -> Result<bool> {
    if *i >= buf.len() {
        return Err(EngineError::Malformed("bool".into()));
    }
    let v = buf[*i] != 0;
    *i += 1;
    Ok(v)
}

pub fn read_u16(buf: &[u8], i: &mut usize) -> Result<u16> {
    if *i + 2 > buf.len() {
        return Err(EngineError::Malformed("u16".into()));
    }
    let v = u16::from_le_bytes(buf[*i..*i + 2].try_into().unwrap());
    *i += 2;
    Ok(v)
}

pub fn read_u32(buf: &[u8], i: &mut usize) -> Result<u32> {
    if *i + 4 > buf.len() {
        return Err(EngineError::Malformed("u32".into()));
    }
    let v = u32::from_le_bytes(buf[*i..*i + 4].try_into().unwrap());
    *i += 4;
    Ok(v)
}

pub fn disc_eq(bytes: &[u8], disc: &[u8; 8]) -> bool {
    bytes.len() >= 8 && bytes[..8] == disc[..]
}

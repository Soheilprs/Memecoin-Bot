use alloy_primitives::keccak256;

/// Solidity CBOR metadata suffix starts near 0xa2 0x64 0x69 0x70 0x66 0x73 ('a' 'i' 'p' 'f' 's').
pub fn strip_solidity_metadata(code: &[u8]) -> &[u8] {
    if code.len() < 4 {
        return code;
    }
    for i in (0..code.len().saturating_sub(7)).rev() {
        if code[i] == 0xa2 && code[i + 1] == 0x64 && code[i + 2] == 0x69 && code[i + 3] == 0x70 {
            return &code[..i];
        }
    }
    code
}

pub fn runtime_hash(code: &[u8]) -> String {
    format!("0x{}", hex::encode(keccak256(code)))
}

pub fn stripped_hash(code: &[u8]) -> String {
    runtime_hash(strip_solidity_metadata(code))
}

pub fn has_op(code: &[u8], op: u8) -> bool {
    let mut i = 0;
    while i < code.len() {
        let b = code[i];
        if b == op {
            return true;
        }
        if (0x60..=0x7f).contains(&b) {
            i += 1 + (b - 0x5f) as usize;
        } else {
            i += 1;
        }
    }
    false
}

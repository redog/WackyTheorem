//! Minimal hex helpers for key blobs and the recovery-key display format.
//! Hand-rolled (~30 lines) rather than a crate dependency; covered by
//! round-trip tests below. Not constant-time — never used to compare
//! secrets, only to encode/decode them.

/// Lowercase hex encoding.
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        out.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
    }
    out
}

/// Strict decode: even length, hex digits only.
pub fn decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    s.as_bytes()
        .chunks(2)
        .map(|pair| {
            let hi = (pair[0] as char).to_digit(16)?;
            let lo = (pair[1] as char).to_digit(16)?;
            Some(((hi << 4) | lo) as u8)
        })
        .collect()
}

/// Decode exactly 32 bytes of user-typed hex, tolerating the separators a
/// human will plausibly paste back: dashes, spaces, newlines, mixed case.
/// Anything else (or a wrong digit count) is rejected.
pub fn decode_key32_lenient(input: &str) -> Option<[u8; 32]> {
    let mut cleaned = String::with_capacity(64);
    for c in input.chars() {
        match c {
            '-' | ' ' | '\t' | '\r' | '\n' => continue,
            c if c.is_ascii_hexdigit() => cleaned.push(c.to_ascii_lowercase()),
            _ => return None,
        }
    }
    if cleaned.len() != 64 {
        return None;
    }
    let v = decode(&cleaned)?;
    let mut out = [0u8; 32];
    out.copy_from_slice(&v);
    Some(out)
}

/// Uppercase, dash-grouped display for the recovery ceremony:
/// `D2AB-12F0-…` (16 groups of 4). Input must be 64 hex chars.
pub fn group_for_display(hex64: &str) -> String {
    debug_assert_eq!(hex64.len(), 64);
    hex64
        .as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap().to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_all_byte_values() {
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(decode(&encode(&bytes)).unwrap(), bytes);
    }

    #[test]
    fn strict_decode_rejects_garbage() {
        assert!(decode("abc").is_none()); // odd length
        assert!(decode("zz").is_none()); // not hex
    }

    #[test]
    fn lenient_decode_accepts_human_formats() {
        let key = [0xAB; 32];
        let hex = encode(&key);
        let displayed = group_for_display(&hex);
        assert_eq!(decode_key32_lenient(&displayed).unwrap(), key);
        assert_eq!(decode_key32_lenient(&hex).unwrap(), key);
        assert_eq!(
            decode_key32_lenient(&format!(" {} \n", displayed.to_lowercase())).unwrap(),
            key
        );
    }

    #[test]
    fn lenient_decode_rejects_wrong_length_and_alphabet() {
        assert!(decode_key32_lenient("abcd").is_none());
        assert!(decode_key32_lenient(&"g".repeat(64)).is_none());
        assert!(decode_key32_lenient(&"a".repeat(66)).is_none());
    }
}

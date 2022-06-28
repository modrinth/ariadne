use thiserror::Error;

/// An error decoding a number from base62.
#[derive(Error, Debug)]
pub enum DecodingError {
    /// Encountered a non-base62 character in a base62 string
    #[error("Invalid character {0:?} in base62 encoding")]
    InvalidBase62(char),
    /// Encountered integer overflow when decoding a base62 id.
    #[error("Base62 decoding overflowed")]
    Overflow,
}

const BASE62_CHARS: [u8; 62] = *b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

pub fn to_base62(mut num: u64) -> String {
    let length = (num as f64).log(62.0).ceil() as usize;
    let mut output = String::with_capacity(length);

    while num > 0 {
        // Could be done more efficiently, but requires byte
        // manipulation of strings & Vec<u8> -> String conversion
        output.insert(0, BASE62_CHARS[(num % 62) as usize] as char);
        num /= 62;
    }
    output
}

pub fn parse_base62(string: &str) -> Result<u64, DecodingError> {
    let mut num: u64 = 0;
    for c in string.chars() {
        let next_digit;
        if c.is_ascii_digit() {
            next_digit = (c as u8 - b'0') as u64;
        } else if c.is_ascii_uppercase() {
            next_digit = 10 + (c as u8 - b'A') as u64;
        } else if c.is_ascii_lowercase() {
            next_digit = 36 + (c as u8 - b'a') as u64;
        } else {
            return Err(DecodingError::InvalidBase62(c));
        }

        // We don't want this panicking or wrapping on integer overflow
        if let Some(n) = num.checked_mul(62).and_then(|n| n.checked_add(next_digit)) {
            num = n;
        } else {
            return Err(DecodingError::Overflow);
        }
    }
    Ok(num)
}

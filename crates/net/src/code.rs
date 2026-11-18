//! A table's code: an IPv4 address and a port in twelve letters a player can read out or
//! type, `K7Q4-MD2X-9HTA`. Crockford's base 32 is used, so the letters that look alike are
//! read alike, and a byte of sum catches a mistyped letter before anything is tried.

use std::net::{Ipv4Addr, SocketAddrV4};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const LETTERS: usize = 12;

/// Letters of a table's short code.
pub const SHORT: usize = 5;

/// A table's short code, five letters a player reads out in a breath: `K7Q4M`. It names the
/// table, not where it is: the table is found by it on the networks this machine is on and
/// through Steam.
pub fn short() -> String {
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()) ^ ((std::process::id() as u128) << 70);
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in seed.to_le_bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (0..SHORT).map(|n| ALPHABET[((hash >> (n * 5)) & 31) as usize] as char).collect()
}

/// A short code as the player typed it, read the forgiving way (case, O for 0, I and L for
/// 1, dashes and spaces): `None` when it is not five letters of the alphabet.
pub fn read_short(text: &str) -> Option<String> {
    let kept: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect();
    (kept.chars().count() == SHORT && kept.bytes().all(|b| ALPHABET.contains(&b))).then_some(kept)
}

/// What is wrong with a code as typed.
#[derive(Debug, PartialEq, Eq)]
pub enum CodeError {
    /// Not twelve letters of the alphabet.
    Shape,
    /// The letters do not add up: one of them is mistyped.
    Sum,
}

fn sum(bytes: &[u8; 6]) -> u8 {
    bytes.iter().fold(0x5Au8, |acc, b| acc.rotate_left(3) ^ b)
}

/// The code of a table at this address.
pub fn encode(address: SocketAddrV4) -> String {
    let ip = address.ip().octets();
    let port = address.port().to_be_bytes();
    let bytes = [ip[0], ip[1], ip[2], ip[3], port[0], port[1]];
    let mut value: u64 = 0;
    for b in bytes {
        value = (value << 8) | b as u64;
    }
    value = (value << 8) | sum(&bytes) as u64;
    // 56 bits in 12 letters of 5 bits: the top 4 bits are zero.
    let mut letters = [0u8; LETTERS];
    for n in (0..LETTERS).rev() {
        letters[n] = ALPHABET[(value & 31) as usize];
        value >>= 5;
    }
    let text = String::from_utf8(letters.to_vec()).unwrap_or_default();
    format!("{}-{}-{}", &text[0..4], &text[4..8], &text[8..12])
}

/// The address a code stands for. Case does not matter, nor do dashes and spaces; O is
/// read as 0 and I and L as 1.
pub fn decode(code: &str) -> Result<SocketAddrV4, CodeError> {
    let mut value: u64 = 0;
    let mut count = 0;
    for ch in code.chars().filter(|c| !c.is_whitespace() && *c != '-') {
        let ch = match ch.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        };
        let Some(at) = ALPHABET.iter().position(|a| *a as char == ch) else { return Err(CodeError::Shape) };
        value = (value << 5) | at as u64;
        count += 1;
    }
    if count != LETTERS || value >> 56 != 0 {
        return Err(CodeError::Shape);
    }
    let check = (value & 0xFF) as u8;
    value >>= 8;
    let mut bytes = [0u8; 6];
    for n in (0..6).rev() {
        bytes[n] = (value & 0xFF) as u8;
        value >>= 8;
    }
    if sum(&bytes) != check {
        return Err(CodeError::Sum);
    }
    Ok(SocketAddrV4::new(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]), u16::from_be_bytes([bytes[4], bytes[5]])))
}

/// Whether what was typed looks like a code rather than an address: twelve letters of the
/// alphabet, with or without dashes, and no dots or colons.
pub fn looks_like_code(text: &str) -> bool {
    let kept: String = text.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    kept.len() == LETTERS && !kept.contains(['.', ':'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_comes_back_from_its_code() {
        for address in ["192.168.1.5:47800", "26.14.3.7:47801", "10.147.17.20:47809", "203.0.113.7:65535", "0.0.0.0:0"] {
            let address: SocketAddrV4 = address.parse().unwrap();
            let code = encode(address);
            assert_eq!(code.len(), 14, "{code}");
            assert_eq!(decode(&code), Ok(address), "{code}");
        }
    }

    #[test]
    fn a_code_is_read_however_it_is_typed() {
        let address: SocketAddrV4 = "192.168.1.5:47800".parse().unwrap();
        let code = encode(address);
        let loose = code.to_lowercase().replace('-', " ").replace('0', "o").replace('1', "l");
        assert_eq!(decode(&loose), Ok(address));
    }

    #[test]
    fn a_mistyped_letter_is_caught() {
        let code = encode("192.168.1.5:47800".parse().unwrap());
        let mut letters: Vec<char> = code.chars().collect();
        let at = 6;
        letters[at] = if letters[at] == 'Z' { 'Y' } else { 'Z' };
        let typo: String = letters.into_iter().collect();
        assert_eq!(decode(&typo), Err(CodeError::Sum));
        assert_eq!(decode("K7Q4-MD2X"), Err(CodeError::Shape));
        assert_eq!(decode("K7Q4-MD2X-9HT!"), Err(CodeError::Shape));
    }

    #[test]
    fn a_short_code_is_five_letters_read_however_typed() {
        let code = short();
        assert_eq!(code.len(), SHORT);
        assert_eq!(read_short(&code.to_lowercase().replace('0', "o").replace('1', "l")), Some(code.clone()));
        assert_eq!(read_short("K7Q4"), None);
        assert_eq!(read_short("192.168.1.5"), None);
        assert_eq!(read_short("k7q-4m"), Some("K7Q4M".to_string()));
    }

    #[test]
    fn codes_and_addresses_are_told_apart() {
        assert!(looks_like_code("K7Q4-MD2X-9HTA"));
        assert!(!looks_like_code("192.168.1.5:47800"));
        assert!(!looks_like_code("localhost"));
    }
}

//! Encrypts API keys before they touch the disk. On Windows the data is bound to the user
//! account (DPAPI): the file is useless when copied to another account or machine.

use std::io;

#[cfg(windows)]
mod imp {
    use std::io;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptProtectData, CryptUnprotectData};

    fn run(input: &[u8], protect: bool) -> io::Result<Vec<u8>> {
        let source = CRYPT_INTEGER_BLOB { cbData: input.len() as u32, pbData: input.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: null_mut() };
        // SAFETY: `source` points at `input`, which outlives the call and is not written to;
        // every optional argument is null; `out` is a valid place for the result.
        let ok = unsafe {
            if protect {
                CryptProtectData(&source, null(), null(), null(), null(), 0, &mut out)
            } else {
                CryptUnprotectData(&source, null_mut(), null(), null(), null(), 0, &mut out)
            }
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: on success the API returns `cbData` bytes at `pbData`, allocated with
        // LocalAlloc and owned by the caller; they are copied out and freed exactly once.
        let bytes = unsafe {
            let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            LocalFree(out.pbData as _);
            bytes
        };
        Ok(bytes)
    }

    pub fn protect(plain: &[u8]) -> io::Result<Vec<u8>> {
        run(plain, true)
    }

    pub fn unprotect(blob: &[u8]) -> io::Result<Vec<u8>> {
        run(blob, false)
    }
}

// Linux support comes with the Linux build; until then the data is stored as is there.
#[cfg(not(windows))]
mod imp {
    use std::io;

    pub fn protect(plain: &[u8]) -> io::Result<Vec<u8>> {
        Ok(plain.to_vec())
    }

    pub fn unprotect(blob: &[u8]) -> io::Result<Vec<u8>> {
        Ok(blob.to_vec())
    }
}

pub fn protect(plain: &[u8]) -> io::Result<Vec<u8>> {
    imp::protect(plain)
}

pub fn unprotect(blob: &[u8]) -> io::Result<Vec<u8>> {
    imp::unprotect(blob)
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn from_hex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_data_round_trips() {
        let secret = b"AIza-not-a-real-key";
        let blob = protect(secret).unwrap();
        assert_eq!(unprotect(&blob).unwrap(), secret);
    }

    #[cfg(windows)]
    #[test]
    fn protected_data_does_not_contain_the_secret() {
        let secret = b"AIza-not-a-real-key";
        let blob = protect(secret).unwrap();
        assert!(!blob.windows(secret.len()).any(|w| w == secret));
    }

    #[cfg(windows)]
    #[test]
    fn tampered_data_is_refused() {
        let mut blob = protect(b"secret").unwrap();
        let last = blob.len() - 1;
        blob[last] ^= 0xff;
        assert!(unprotect(&blob).is_err());
    }

    #[test]
    fn hex_round_trips_and_rejects_garbage() {
        assert_eq!(from_hex(&to_hex(&[0, 15, 255])).unwrap(), [0, 15, 255]);
        assert_eq!(from_hex("abc"), None);
        assert_eq!(from_hex("zz"), None);
    }
}

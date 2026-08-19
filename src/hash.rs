use sha2::{Digest, Sha256};

pub fn digest_hex(bytes: &[u8]) -> String {
    let hash = digest(bytes);
    let mut result = String::with_capacity(64);
    for byte in hash {
        use std::fmt::Write;
        write!(result, "{byte:02x}").unwrap();
    }
    result
}

pub(crate) fn digest(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::digest_hex;

    #[test]
    fn matches_known_sha256_vector() {
        assert_eq!(
            digest_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn matches_empty_sha256_vector() {
        assert_eq!(
            digest_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}

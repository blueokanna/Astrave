//! RFC 4648 base64 (standard alphabet, padded).

use alloc::{string::String, vec::Vec};
use base64::{
    Engine, alphabet,
    engine::{GeneralPurpose, GeneralPurposeConfig},
};

// v1 readers accept nonzero unused bits in the final padded quartet.
const CODEC: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true),
);

pub fn encode(input: &[u8]) -> String {
    CODEC.encode(input)
}

pub fn decode(input: &[u8]) -> Option<Vec<u8>> {
    CODEC.decode(input.trim_ascii()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc4648_vectors() {
        for (plain, encoded) in [
            (&b""[..], ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg=="),
            (b"fooba", "Zm9vYmE="),
            (b"foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode(plain), encoded);
            assert_eq!(decode(encoded.as_bytes()).unwrap(), plain);
        }
    }

    #[test]
    fn rejects_malformed() {
        assert!(decode(b"Zg=").is_none());
        assert!(decode(b"Zg===").is_none());
        assert!(decode(b"!!!!").is_none());
        assert!(decode(b"Zg==Zm8=").is_none());
        assert_eq!(decode(b" \tZh==\r\n").as_deref(), Some(b"f".as_slice()));
    }
}

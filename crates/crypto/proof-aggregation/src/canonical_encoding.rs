/// Accept only the exact canonical serialization of a decoded value.
pub fn canonical_encoding_matches(input: &[u8], encoded: &[u8]) -> bool {
    input == encoded
}

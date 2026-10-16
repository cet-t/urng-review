fn chacha(key: [u8; 32], nonce: [u8; 12], counter: u32) -> ChaCha20 {
    ChaCha20::from_key_nonce(key, nonce, counter)
}
fn salsa(key: [u8; 32], nonce: [u8; 8], counter: u64) -> Salsa20 {
    Salsa20::from_key_nonce(key, nonce, counter)
}

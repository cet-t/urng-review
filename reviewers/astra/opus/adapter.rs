fn chacha(key: [u8; 32], nonce: [u8; 12], counter: u32) -> ChaCha20 {
    let words = std::array::from_fn(|i| u32::from_le_bytes(key[i*4..i*4+4].try_into().unwrap()));
    let stream = u64::from_le_bytes(nonce[4..12].try_into().unwrap());
    let counter = ((u32::from_le_bytes(nonce[..4].try_into().unwrap()) as u64) << 32) | counter as u64;
    let mut rng = ChaCha20::from_key(words, stream);
    rng.set_block_counter(counter);
    rng
}
fn salsa(key: [u8; 32], nonce: [u8; 8], counter: u64) -> Salsa20 {
    let words = std::array::from_fn(|i| u32::from_le_bytes(key[i*4..i*4+4].try_into().unwrap()));
    let mut rng = Salsa20::from_key(words, u64::from_le_bytes(nonce));
    rng.astra_set_block_counter(counter);
    rng
}

use urng::Rng;
#[path = "support/sol_cipher_reference.rs"]
mod reference;

macro_rules! make_chacha_adapter {
    ($name:ident, $ty:ident) => {
fn $name(key: [u8;32], nonce: u64, counter: u64) -> urng::$ty {
    let mut iv = [0u8;12];
    iv[4..].copy_from_slice(&nonce.to_le_bytes());
    urng::$ty::from_key_nonce(key,iv,counter.try_into().unwrap())
}
    }
}
macro_rules! make_salsa_adapter {
    ($name:ident, $ty:ident) => {
fn $name(key: [u8;32], nonce: u64, counter: u64) -> urng::$ty {
    urng::$ty::from_key_nonce(key,nonce.to_le_bytes(),counter)
}
    }
}
make_chacha_adapter!(chacha8, ChaCha8);
make_chacha_adapter!(chacha20, ChaCha20);
make_salsa_adapter!(salsa8, Salsa8);
make_salsa_adapter!(salsa20, Salsa20);

macro_rules! cipher_test {
    ($test:ident, $make:ident, $n:expr, $salsa:expr) => {
        #[test]
        fn $test() {
            let key = std::array::from_fn(|i| (i as u8).wrapping_mul(7));
            for counter in [0, 7, u32::MAX as u64-1] {
                let mut rng = $make(key, 0x123456789abcdef0, counter);
                for i in 0..2 {
                    let got: [u32;16] = std::array::from_fn(|_| rng.nextu());
                    assert_eq!(got, reference::block(key,0x123456789abcdef0,counter+i,$n,$salsa));
                }
            }
        }
    }
}
cipher_test!(chacha8_independent_blocks, chacha8, 8, false);
cipher_test!(chacha20_independent_blocks, chacha20, 20, false);
cipher_test!(salsa8_independent_blocks, salsa8, 8, true);
cipher_test!(salsa20_independent_blocks, salsa20, 20, true);

#[test]
fn chacha_key_does_not_appear_in_debug_output() {
    let key = [0xa7;32]; // Synthetic review key, never a real secret.
    let c = format!("{:?}", chacha20(key,0,0));
    let decimal_word = u32::from_le_bytes([0xa7;4]).to_string();
    assert!(!c.contains(&decimal_word), "ChaCha Debug exposes key: {c}");
}

#[test]
fn salsa_key_does_not_appear_in_debug_output() {
    let key = [0xa7;32];
    let s = format!("{:?}", salsa20(key,0,0));
    assert!(!s.contains(&u32::from_le_bytes([0xa7;4]).to_string()), "Salsa Debug exposes key: {s}");
}

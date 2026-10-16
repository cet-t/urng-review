// Independently expressed block transforms from RFC 8439 and D. J. Bernstein's
// Salsa20 specification. No generator code from urng is used here.
fn qr(x: &mut [u32;16], p: [usize;4], salsa: bool) {
    let [a,b,c,d] = p;
    if salsa {
        x[b] ^= x[a].wrapping_add(x[d]).rotate_left(7);
        x[c] ^= x[b].wrapping_add(x[a]).rotate_left(9);
        x[d] ^= x[c].wrapping_add(x[b]).rotate_left(13);
        x[a] ^= x[d].wrapping_add(x[c]).rotate_left(18);
    } else {
        for (r,s) in [(16,12), (8,7)] {
            x[a] = x[a].wrapping_add(x[b]); x[d] = (x[d] ^ x[a]).rotate_left(r);
            x[c] = x[c].wrapping_add(x[d]); x[b] = (x[b] ^ x[c]).rotate_left(s);
        }
    }
}
pub fn block(key: [u8;32], nonce: u64, counter: u64, rounds: usize, salsa: bool) -> [u32;16] {
    let key: [u32;8] = std::array::from_fn(|i| u32::from_le_bytes(key[i*4..i*4+4].try_into().unwrap()));
    let sigma = [0x61707865,0x3320646e,0x79622d32,0x6b206574];
    let mut input = [0u32;16];
    if salsa {
        for i in 0..4 { input[i*5] = sigma[i]; }
        input[1..5].copy_from_slice(&key[..4]); input[11..15].copy_from_slice(&key[4..]);
        input[6] = nonce as u32; input[7] = (nonce >> 32) as u32;
        input[8] = counter as u32; input[9] = (counter >> 32) as u32;
    } else {
        input[..4].copy_from_slice(&sigma); input[4..12].copy_from_slice(&key);
        input[12] = counter as u32; input[13] = (counter >> 32) as u32;
        input[14] = nonce as u32; input[15] = (nonce >> 32) as u32;
    }
    let mut x = input;
    for _ in 0..rounds/2 {
        if salsa {
            for i in 0..4 { qr(&mut x, std::array::from_fn(|j| (i*5+j*4)%16), true); }
            for i in 0..4 { qr(&mut x, std::array::from_fn(|j| i*4+(i+j)%4), true); }
        } else {
            for i in 0..4 { qr(&mut x, [i,i+4,i+8,i+12], false); }
            for i in 0..4 { qr(&mut x, [i,4+(i+1)%4,8+(i+2)%4,12+(i+3)%4], false); }
        }
    }
    std::array::from_fn(|i| x[i].wrapping_add(input[i]))
}

#[test]
fn chacha_reference_rfc8439_known_answer() {
    let key = std::array::from_fn(|i| i as u8);
    assert_eq!(block(key,0x4a000000,0x0900000000000001,20,false),
        [0xe4e7f110,0x15593bd1,0x1fdd0f50,0xc47120a3,0xc7f4d1c7,0x0368c033,0x9aaa2204,0x4e6cd4c3,
         0x466482d2,0x09aa9f07,0x05d7c214,0xa2028bd9,0xd19c12b5,0xb94e16de,0xe883d0cb,0x4e3c50a2]);
}

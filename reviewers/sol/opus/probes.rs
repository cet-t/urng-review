//! Independent review tests. Failures are evidence; do not change expected values.
use urng::{Rng, Word, Sampler, Alias32, Alias64};

struct Const32(u32);
impl Rng for Const32 { type Word = u32; fn nextu(&mut self) -> u32 { self.0 } }
struct Const64(u64);
impl Rng for Const64 { type Word = u64; fn nextu(&mut self) -> u64 { self.0 } }

#[test]
fn alias32_small_valid_weight_never_selects_zero() {
    for weight in [f32::from_bits(1), f32::MIN_POSITIVE] {
        let mut rng = Const32(u32::MAX);
        let mut sampler = Alias32::new(&mut rng, &[weight, 0.0]);
        assert_eq!(sampler.sample(), 0, "positive weight={weight:e}");
    }
}

#[test]
fn alias64_small_valid_weight_never_selects_zero() {
    for weight in [f64::from_bits(1), f64::MIN_POSITIVE] {
        let mut rng = Const64(u64::MAX);
        let mut sampler = Alias64::new(&mut rng, &[weight, 0.0]);
        assert_eq!(sampler.sample(), 0, "positive weight={weight:e}");
    }
}

#[test]
fn alias32_rejects_invalid_weights() {
    for weights in [&[-1.0, 2.0][..], &[f32::NAN, 1.0], &[f32::INFINITY, 1.0], &[0.0, 0.0]] {
        let result = std::panic::catch_unwind(|| {
            let mut rng = Const32(u32::MAX);
            let _ = Alias32::new(&mut rng, weights);
        });
        assert!(result.is_err(), "accepted invalid weights {weights:?}");
    }
}

#[test]
fn alias64_rejects_invalid_weights() {
    for weights in [&[-1.0, 2.0][..], &[f64::NAN, 1.0], &[f64::INFINITY, 1.0], &[0.0, 0.0]] {
        let result = std::panic::catch_unwind(|| {
            let mut rng = Const64(u64::MAX);
            let _ = Alias64::new(&mut rng, weights);
        });
        assert!(result.is_err(), "accepted invalid weights {weights:?}");
    }
}

#[test]
fn float32_finite_extreme_endpoints_remain_in_range() {
    for word in [0, 1, 1 << 31, u32::MAX] {
        let got = word.to_randf(-f32::MAX, f32::MAX);
        assert!(got.is_finite() && got >= -f32::MAX && got < f32::MAX, "word={word:x} got={got}");
    }
}

#[test]
fn float64_finite_extreme_endpoints_remain_in_range() {
    for word in [0, 1, 1 << 63, u64::MAX] {
        let got = word.to_randf(-f64::MAX, f64::MAX);
        assert!(got.is_finite() && got >= -f64::MAX && got < f64::MAX, "word={word:x} got={got}");
    }
}

// Seed expansion is reproduced only to discover the constructor's documented
// key layout; the Threefry reference below is independent and KAT-checked.
fn seed_words(seed: u32, n: usize) -> Vec<u32> {
    let mut state = seed;
    (0..n).map(|_| {
        state = state.wrapping_add(0x9e3779b9);
        let mut x = state as u64;
        x = (x ^ (x >> 16)).wrapping_mul(0xff51afd7ed558ccd);
        x = (x ^ (x >> 16)).wrapping_mul(0xc4ceb9fe1a85ec53);
        (x ^ (x >> 16)) as u32
    }).collect()
}

// Random123 Threefry4x32-20, using a loop and published rotation schedule.
// https://github.com/DEShawResearch/random123/blob/main/include/Random123/threefry.h
fn tf4(counter: [u32; 4], key: [u32; 4]) -> [u32; 4] {
    let rotations = [[10,26], [11,21], [13,27], [23,5], [6,20], [17,11], [25,10], [18,20]];
    let k = [key[0], key[1], key[2], key[3], key.iter().fold(0x1bd11bda, |a, b| a ^ b)];
    let mut x = counter;
    for group in 0..=5 {
        for j in 0..4 { x[j] = x[j].wrapping_add(k[(group + j) % 5]); }
        x[3] = x[3].wrapping_add(group as u32);
        if group == 5 { break; }
        for round in group * 4..group * 4 + 4 {
            let r = rotations[round % 8];
            x[0] = x[0].wrapping_add(x[1]);
            x[1] = x[1].rotate_left(r[0]) ^ x[0];
            x[2] = x[2].wrapping_add(x[3]);
            x[3] = x[3].rotate_left(r[1]) ^ x[2];
            x.swap(1, 3);
        }
    }
    x
}

#[test]
fn independent_threefry_reference_matches_official_kats() {
    // https://github.com/DEShawResearch/random123/blob/main/tests/kat_vectors
    assert_eq!(tf4([0;4], [0;4]), [0x9c6ca96a,0xe17eae66,0xfc10ecd4,0x5256a7d8]);
    assert_eq!(tf4([u32::MAX;4], [u32::MAX;4]), [0x2a881696,0x57012287,0xf6c7446e,0xa16a6732]);
    assert_eq!(tf4([0x243f6a88,0x85a308d3,0x13198a2e,0x03707344],
                   [0xa4093822,0x299f31d0,0x082efa98,0xec4e6c89]),
                   [0x59cd1dbb,0xb8879579,0x86b5d00c,0xac8b6d84]);
}

#[test]
fn scalar_threefry_matches_random123() {
    for seed in [0, 1, 0xfedcba98] {
        let keys = seed_words(seed, 4);
        let key: [u32;4] = keys.try_into().unwrap();
        let mut rng = urng::Threefry32x4::new(seed);
        for block in 0..20 {
            let got: [u32;4] = std::array::from_fn(|_| rng.nextu());
            assert_eq!(got, tf4([block,0,0,0], key), "seed={seed} block={block}");
        }
    }
}

macro_rules! wide_threefry_test {
    ($test:ident, $ty:ident, $n:expr) => {
        #[test]
        fn $test() {
            use urng::wide::{RngW, $ty};
            let words = seed_words(1, 4 * $n);
            let mut rng = $ty::new(1);
            for block in 0..3 {
                let output: [[u32;$n];4] = std::array::from_fn(|_| rng.nextu());
                for lane in 0..$n {
                    let key = std::array::from_fn(|j| words[j * $n + lane]);
                    let got = std::array::from_fn(|j| output[j][lane]);
                    assert_eq!(got, tf4([block,0,0,0], key), "block={block} lane={lane}");
                }
            }
        }
    }
}
wide_threefry_test!(wide_threefry_x4_matches_random123, Threefry32x4x4, 4);
wide_threefry_test!(wide_threefry_x8_matches_random123, Threefry32x4x8, 8);
wide_threefry_test!(wide_threefry_x16_matches_random123, Threefry32x4x16, 16);

fn sm64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

#[test]
fn xoshiro256pp_matches_author_reference() {
    // https://prng.di.unimi.it/xoshiro256plusplus.c
    for seed in [0, 1, u64::MAX] {
        let mut sm = seed;
        let mut s: [u64;4] = std::array::from_fn(|_| sm64(&mut sm));
        let mut rng = urng::Xoshiro256Pp::new(seed);
        for _ in 0..1000 {
            let want = s[0].wrapping_add(s[3]).rotate_left(23).wrapping_add(s[0]);
            let t = s[1] << 17;
            s[2] ^= s[0]; s[3] ^= s[1]; s[1] ^= s[2]; s[0] ^= s[3]; s[2] ^= t; s[3] = s[3].rotate_left(45);
            assert_eq!(rng.nextu(), want);
        }
    }
}

#[test]
fn cabi_splitmix_continues_across_calls() {
    unsafe {
        let ptr = urng::splitmix64_new(1);
        let mut expected = 1;
        for n in [0, 1, 3, 17, 131_073, 5] {
            let mut out = vec![0; n + 2];
            out[0] = 0xbadc0ffee;
            out[n+1] = 0xbadc0ffee;
            urng::splitmix64_next_u64s(ptr, out.as_mut_ptr().add(1), n);
            assert_eq!(out[0], 0xbadc0ffee);
            assert_eq!(out[n+1], 0xbadc0ffee);
            for word in &out[1..n+1] { assert_eq!(*word, sm64(&mut expected)); }
        }
        urng::splitmix64_free(ptr);
    }
}

#[test]
fn cabi_threefry_continues_across_odd_calls() {
    unsafe {
        let ptr = urng::threefry32x4_new(1);
        let mut scalar = urng::Threefry32x4::new(1);
        for n in [0, 1, 3, 17, 131_073, 5] {
            let mut out = vec![0; n];
            urng::threefry32x4_next_u32s(ptr, out.as_mut_ptr(), n);
            for word in out { assert_eq!(word, scalar.nextu()); }
        }
        urng::threefry32x4_free(ptr);
    }
}

#[test]
fn full_width_integer_endpoints_match_bijection() {
    for x in [0, 1, 1 << 31, u32::MAX] { assert_eq!(x.to_randi(i32::MIN,i32::MAX), (x as i32).wrapping_add(i32::MIN)); }
    for x in [0, 1, 1 << 63, u64::MAX] { assert_eq!(x.to_randi(i64::MIN,i64::MAX), (x as i64).wrapping_add(i64::MIN)); }
}

macro_rules! zero_lane_test {
    ($test:ident, $ty:ident) => {
        #[test]
        fn $test() {
            use urng::wide::{RngW, $ty};
            // First SplitMix32 Weyl state is exactly zero at this legal seed.
            let mut rng = $ty::new(0u32.wrapping_sub(0x9e3779b9));
            let values: Vec<_> = (0..16).map(|_| rng.nextu()[0]).collect();
            assert!(values.iter().any(|&x| x != 0), "lane 0 is stuck: {values:?}");
        }
    }
}
zero_lane_test!(wide_xorshift32_x4_avoids_zero_state, Xorshift32x4);
zero_lane_test!(wide_xorshift32_x8_avoids_zero_state, Xorshift32x8);
zero_lane_test!(wide_xorshift32_x16_avoids_zero_state, Xorshift32x16);

fn xoroshiro_step(s: &mut [u64;2], pp: bool) -> u64 {
    let [a,b] = *s;
    let t = a ^ b;
    if pp {
        *s = [a.rotate_left(49)^t^(t<<21), t.rotate_left(28)];
        a.wrapping_add(b).rotate_left(17).wrapping_add(a)
    } else {
        *s = [a.rotate_left(24)^t^(t<<16), t.rotate_left(37)];
        a.wrapping_mul(5).rotate_left(7).wrapping_mul(9)
    }
}

fn xoroshiro_jump(s: &mut [u64;2], poly: [u64;2], pp: bool) {
    let mut next = [0;2];
    for word in poly {
        for bit in 0..64 {
            if (word>>bit)&1 != 0 { next[0] ^= s[0]; next[1] ^= s[1]; }
            let _ = xoroshiro_step(s,pp);
        }
    }
    *s = next;
}

macro_rules! jump_test {
    ($test:ident, $ty:ident, $pp:expr, $jump:expr, $long:expr) => {
        #[test]
        fn $test() {
            for seed in [0,1,u64::MAX] {
                let mut sg = seed;
                let mut s = [sm64(&mut sg),sm64(&mut sg)];
                let mut rng = urng::$ty::new(seed);
                for _ in 0..20 { assert_eq!(rng.nextu(), xoroshiro_step(&mut s,$pp)); }
                rng.jump(); xoroshiro_jump(&mut s,$jump,$pp);
                for _ in 0..20 { assert_eq!(rng.nextu(), xoroshiro_step(&mut s,$pp)); }
                rng.long_jump(); xoroshiro_jump(&mut s,$long,$pp);
                for _ in 0..20 { assert_eq!(rng.nextu(), xoroshiro_step(&mut s,$pp)); }
            }
        }
    }
}
jump_test!(xoroshiro128pp_jumps_match_author, Xoroshiro128Pp, true,
    [0x2bd7a6a6e99c2ddc,0x0992ccaf6a6fca05], [0x360fd5f2cf8d5d99,0x9c6e6877736c46e3]);
jump_test!(xoroshiro128ss_jumps_match_author, Xoroshiro128Ss, false,
    [0xdf900294d8f554a5,0x170865df4b3201fc], [0xd2a98b26625eee7b,0xdddf9b1090aa7ac1]);

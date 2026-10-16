//! Targeted probes used as minimal reproductions in the review.
//! usage: probe <name>
use urng::*;

fn first<R: Rng>(mut r: R, n: usize) -> Vec<R::Word>
where
    R::Word: Copy,
{
    (0..n).map(|_| r.nextu()).collect()
}

/// Seeds 2k and 2k+1 produce identical streams?
fn seed_pairs() {
    macro_rules! pair32 {
        ($($t:ty),+) => {$(
            let a = first(<$t>::new(2), 64);
            let b = first(<$t>::new(3), 64);
            let c = first(<$t>::new(0x1234_5678), 64);
            let d = first(<$t>::new(0x1234_5679), 64);
            println!("{:<14} new(2)==new(3): {:<5}  new(0x12345678)==new(0x12345679): {}", stringify!($t), a == b, c == d);
        )+};
    }
    macro_rules! pair64 {
        ($($t:ty),+) => {$(
            let a = first(<$t>::new(2), 64);
            let b = first(<$t>::new(3), 64);
            let c = first(<$t>::new(1u64 << 40), 64);
            let d = first(<$t>::new((1u64 << 40) | 1), 64);
            println!("{:<14} new(2)==new(3): {:<5}  new(2^40)==new(2^40+1): {}", stringify!($t), a == b, c == d);
        )+};
    }
    pair32!(SplitMix32, Xoshiro128Pp, Xoshiro128Ss, Xoroshiro64Ss, Jsf32, Sfc32, Xorshift32,
        Xorshift128, Xorwow, Mt19937, Pcg32, Philox32, Threefry32x4, Threefry32x2, Squares32,
        Sfmt19937, ChaCha20, Salsa20);
    pair64!(SplitMix64, Xoshiro256Pp, Xoshiro256Ss, Xoroshiro128Pp, Xoroshiro128Ss, Sfc64,
        Xorshift64, Mt1993764, Sfmt1993764, Philox64, Philox4x64, Cet64, Cet256, Threefish256,
        Biski64, WyHash64, TwistedGFSR);
    // TwistedGFSR: does the high half of the 64-bit seed matter?
    let a = first(TwistedGFSR::new(5), 64);
    let b = first(TwistedGFSR::new(5 | (0xDEAD_BEEFu64 << 32)), 64);
    println!("TwistedGFSR    new(5)==new(5 | 0xDEADBEEF<<32): {}", a == b);
    // wide generators
    use urng::wide::RngW;
    let mut a = urng::wide::Squares32x8::new(2);
    let mut b = urng::wide::Squares32x8::new(3);
    println!("wide::Squares32x8 new(2)==new(3): {}", (0..8).all(|_| a.nextu() == b.nextu()));
    let mut a = urng::wide::Pcg32x8::new(2);
    let mut b = urng::wide::Pcg32x8::new(3);
    println!("wide::Pcg32x8 new(2)==new(3): {}", (0..8).all(|_| a.nextu() == b.nextu()));
    let mut a = urng::wide::Xoshiro128Ppx8::new(2);
    let mut b = urng::wide::Xoshiro128Ppx8::new(3);
    println!("wide::Xoshiro128Ppx8 new(2)==new(3): {}", (0..8).all(|_| a.nextu() == b.nextu()));
    // C ABI
    unsafe {
        let mut x = [0u32; 8];
        let mut y = [0u32; 8];
        let h = xoshiro128pp_new(2);
        xoshiro128pp_next_u32s(h, x.as_mut_ptr(), 8);
        xoshiro128pp_free(h);
        let h = xoshiro128pp_new(3);
        xoshiro128pp_next_u32s(h, y.as_mut_ptr(), 8);
        xoshiro128pp_free(h);
        println!("C ABI xoshiro128pp_new(2) == xoshiro128pp_new(3): {}", x == y);
    }
}

/// Brute force: which u32 seeds give Xorshift32 an all-zero state?
fn xorshift32_zero() {
    for seed in [0u32, 1, 2] {
        let _ = seed;
    }
    // Xorshift32::new(seed) state = SplitMix32 first output. Search all seeds by
    // constructing the generator (no internal API used) and checking whether
    // the generator is stuck at zero.
    let threads = 16u64;
    let found = std::sync::Mutex::new(vec![]);
    std::thread::scope(|s| {
        for t in 0..threads {
            let found = &found;
            s.spawn(move || {
                let lo = (t << 32) / threads;
                let hi = ((t + 1) << 32) / threads;
                for seed in lo..hi {
                    let mut r = Xorshift32::new(seed as u32);
                    if r.nextu() == 0 && r.nextu() == 0 {
                        found.lock().unwrap().push(seed as u32);
                    }
                }
            });
        }
    });
    let f = found.into_inner().unwrap();
    println!("Xorshift32 seeds with an all-zero (stuck) state: {:x?}", f);
    for &s in &f {
        let mut r = Xorshift32::new(s);
        let v: Vec<u32> = (0..8).map(|_| r.nextu()).collect();
        println!("  Xorshift32::new({s:#x}) -> {v:?}");
        let mut w = urng::wide::Xorshift32x4::new(s);
        println!("  wide::Xorshift32x4::new({s:#x}).nextu() -> {:?}", urng::wide::RngW::nextu(&mut w));
        unsafe {
            let h = xorshift32_new(s);
            let mut out = [7u32; 8];
            xorshift32_next_u32s(h, out.as_mut_ptr(), 8);
            xorshift32_free(h);
            println!("  C ABI xorshift32_new({s:#x}) -> {out:?}");
        }
    }
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("seed_pairs") => seed_pairs(),
        Some("xorshift32_zero") => xorshift32_zero(),
        other => panic!("unknown probe {other:?}"),
    }
}

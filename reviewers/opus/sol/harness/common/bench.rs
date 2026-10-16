//! Throughput benchmark, compiled identically against base 1.0.0, Sol and Astra.
//! usage: bench [filter]   -> CSV lines: name,unit,median_ns_per_unit,min,max
//! Each case: 7 timed repetitions of >= ~150 ms each, median reported.
use std::hint::black_box;
use std::time::Instant;
use urng::*;

fn measure(name: &str, units_per_iter: u64, mut f: impl FnMut() -> u64) {
    // calibrate
    let mut iters = 1u64;
    loop {
        let t = Instant::now();
        let mut acc = 0u64;
        for _ in 0..iters {
            acc = acc.wrapping_add(f());
        }
        black_box(acc);
        if t.elapsed().as_millis() >= 150 || iters > (1 << 40) {
            break;
        }
        iters *= 2;
    }
    let mut samples = vec![];
    for _ in 0..7 {
        let t = Instant::now();
        let mut acc = 0u64;
        for _ in 0..iters {
            acc = acc.wrapping_add(f());
        }
        black_box(acc);
        samples.push(t.elapsed().as_nanos() as f64 / (iters * units_per_iter) as f64);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("{name},ns/word,{:.4},{:.4},{:.4}", samples[3], samples[0], samples[6]);
}

macro_rules! scalar {
    ($filter:expr, $($t:ty: $seed:expr),+) => {$(
        let name = concat!("rust::", stringify!($t), "::nextu");
        if name.contains($filter) {
            let mut r = <$t>::new($seed);
            measure(name, 1024, || { let mut a = 0u64; for _ in 0..1024 { a = a.wrapping_add(Rng::nextu(&mut r) as u64); } a });
        }
    )+};
}

fn main() {
    let filter = std::env::args().nth(1).unwrap_or_default();
    let filter = filter.as_str();
    scalar!(filter, SplitMix64: 1, Xoshiro256Pp: 1, Xoshiro128Pp: 1, Sfc32: 1, Sfc64: 1, Pcg32: 1,
        Jsf32: 1, Mt19937: 1, Sfmt19937: 1, Mt1993764: 1, Philox32: 1, Philox64: 1,
        Threefry32x4: 1, Threefish256: 1, Squares32: 1, ChaCha8: 1, ChaCha20: 1, Salsa20: 1,
        Biski64: 1, Xoroshiro128Pp: 1, TwistedGFSR: 1);

    // ranged / float draws
    if "rust::Sfc32::randi(0,999)".contains(filter) {
        let mut r = Sfc32::new(1);
        measure("rust::Sfc32::randi(0,999)", 1024, || { let mut a = 0u64; for _ in 0..1024 { a = a.wrapping_add(r.randi(0, 999) as u64); } a });
    }
    if "rust::Sfc32::randi(-5,7)".contains(filter) {
        let mut r = Sfc32::new(1);
        measure("rust::Sfc32::randi(-5,7)", 1024, || { let mut a = 0u64; for _ in 0..1024 { a = a.wrapping_add(r.randi(-5, 7) as u64); } a });
    }
    if "rust::Xoshiro256Pp::randi(0,999)".contains(filter) {
        let mut r = Xoshiro256Pp::new(1);
        measure("rust::Xoshiro256Pp::randi(0,999)", 1024, || { let mut a = 0u64; for _ in 0..1024 { a = a.wrapping_add(r.randi(0, 999) as u64); } a });
    }
    if "rust::Sfc32::randf(0,1)".contains(filter) {
        let mut r = Sfc32::new(1);
        measure("rust::Sfc32::randf(0,1)", 1024, || { let mut a = 0f32; for _ in 0..1024 { a += r.randf(0.0, 1.0); } a.to_bits() as u64 });
    }
    if "rust::Xoshiro256Pp::randf(0,1)".contains(filter) {
        let mut r = Xoshiro256Pp::new(1);
        measure("rust::Xoshiro256Pp::randf(0,1)", 1024, || { let mut a = 0f64; for _ in 0..1024 { a += r.randf(0.0, 1.0); } a.to_bits() })
    }
    if "rust::Sfc32::nextf".contains(filter) {
        let mut r = Sfc32::new(1);
        measure("rust::Sfc32::nextf", 1024, || { let mut a = 0f32; for _ in 0..1024 { a += r.nextf(); } a.to_bits() as u64 });
    }
    if "rust::Sfc64::shuffle(1M)".contains(filter) {
        let mut r = Sfc64::new(1);
        let mut v: Vec<u32> = (0..1_000_000).collect();
        measure("rust::Sfc64::shuffle(1M)", 1_000_000, || { r.shuffle(&mut v).unwrap(); v[0] as u64 });
    }
    if "rust::Alias32::sample(16)".contains(filter) {
        let mut r = Sfc32::new(1);
        let w: Vec<f32> = (1..=16).map(|x| x as f32).collect();
        let mut s = Alias32::new(&mut r, &w);
        measure("rust::Alias32::sample(16)", 1024, || { let mut a = 0u64; for _ in 0..1024 { a = a.wrapping_add(s.sample() as u64); } a });
    }

    #[cfg(target_arch = "x86_64")]
    simd(filter);
    wide(filter);
    cabi(filter);
}

#[cfg(target_arch = "x86_64")]
fn simd(filter: &str) {
    unsafe fn sum<V>(v: V) -> u64 {
        let a: [u64; 8] = unsafe { std::mem::transmute_copy(&v) }; // sums all 512 bits (or the low 512 bits)
        a.iter().fold(0u64, |x, y| x.wrapping_add(*y))
    }
    if "simd::Sfc32x16::nextu".contains(filter) {
        let mut r = unsafe { Sfc32x16::new(1) };
        measure("simd::Sfc32x16::nextu", 16 * 64, || { let mut a = 0u64; for _ in 0..64 { a = a.wrapping_add(r.nextu().iter().fold(0u64, |x, y| x.wrapping_add(*y as u64))); } a });
    }
    if "simd::Sfc32x8::nextu".contains(filter) {
        let mut r = unsafe { Sfc32x8::new(1) };
        measure("simd::Sfc32x8::nextu", 8 * 64, || { let mut a = 0u64; for _ in 0..64 { a = a.wrapping_add(r.nextu().iter().fold(0u64, |x, y| x.wrapping_add(*y as u64))); } a });
    }
    if "simd::Xoshiro128Ppx16::nextuv".contains(filter) {
        let mut r = unsafe { Xoshiro128Ppx16::new(1) };
        measure("simd::Xoshiro128Ppx16::nextuv", 16 * 64, || { let mut a = 0u64; for _ in 0..64 { a = a.wrapping_add(unsafe { sum(r.nextuv()) }); } a });
    }
    if "simd::Jsf32x16::nextuv".contains(filter) {
        let mut r = unsafe { Jsf32x16::new(1) };
        measure("simd::Jsf32x16::nextuv", 16 * 64, || { let mut a = 0u64; for _ in 0..64 { a = a.wrapping_add(unsafe { sum(r.nextuv()) }); } a });
    }
    if "simd::SplitMix32x16::nextuv".contains(filter) {
        let mut r = unsafe { SplitMix32x16::new(1) };
        measure("simd::SplitMix32x16::nextuv", 16 * 64, || { let mut a = 0u64; for _ in 0..64 { a = a.wrapping_add(unsafe { sum(r.nextuv()) }); } a });
    }
    if "simd::Pcg32x8::nextu".contains(filter) {
        let mut r = unsafe { Pcg32x8::new(1) };
        measure("simd::Pcg32x8::nextu", 8 * 64, || { let mut a = 0u64; for _ in 0..64 { a = a.wrapping_add(unsafe { r.nextu() }.iter().fold(0u64, |x, y| x.wrapping_add(*y as u64))); } a });
    }
    if "simd::Philox32x4x4::nextu".contains(filter) {
        let mut r = unsafe { Philox32x4x4::new(1) };
        measure("simd::Philox32x4x4::nextu", 16 * 64, || { let mut a = 0u64; for _ in 0..64 { a = a.wrapping_add(unsafe { r.nextu() }.iter().fold(0u64, |x, y| x.wrapping_add(*y as u64))); } a });
    }
    if "simd::Squares32x8::nextu".contains(filter) {
        let mut r = unsafe { Squares32x8::new(1) };
        measure("simd::Squares32x8::nextu", 8 * 64, || { let mut a = 0u64; for _ in 0..64 { a = a.wrapping_add(unsafe { r.nextu() }.iter().fold(0u64, |x, y| x.wrapping_add(*y as u64))); } a });
    }
    if "simd::Biski64x8::nextu".contains(filter) {
        let mut r = Biski64x8::new(1);
        measure("simd::Biski64x8::nextu", 8 * 64, || { let mut a = 0u64; for _ in 0..64 { a = a.wrapping_add(unsafe { r.nextu() }.iter().fold(0u64, |x, y| x.wrapping_add(*y))); } a });
    }
    if "simd::Sfc64x8::nextu".contains(filter) {
        let mut r = Sfc64x8::new(1);
        measure("simd::Sfc64x8::nextu", 8 * 64, || { let mut a = 0u64; for _ in 0..64 { a = a.wrapping_add(unsafe { r.nextu() }.iter().fold(0u64, |x, y| x.wrapping_add(*y))); } a });
    }
    if "simd::Cet64x8::nextu".contains(filter) {
        let mut r = Cet64x8::new(1);
        measure("simd::Cet64x8::nextu", 8 * 64, || { let mut a = 0u64; for _ in 0..64 { a = a.wrapping_add(unsafe { r.nextu() }.iter().fold(0u64, |x, y| x.wrapping_add(*y))); } a });
    }
}

fn wide(filter: &str) {
    use urng::wide::RngW;
    macro_rules! w {
        ($($t:ident: $n:expr),+) => {$(
            let name = concat!("wide::", stringify!($t), "::nextu");
            if name.contains(filter) {
                let mut r = urng::wide::$t::new(1);
                measure(name, $n * 64, || { let mut a = 0u64; for _ in 0..64 { a = a.wrapping_add(r.nextu().iter().fold(0u64, |x, y| x.wrapping_add(*y as u64))); } a });
            }
        )+};
    }
    w!(Xoshiro128Ppx16: 16, Sfc32x16: 16, SplitMix32x16: 16, Jsf32x16: 16, Threefry32x4x16: 16, Pcg32x16: 16);
    if "wide::Xoshiro128Ppx16::randi(0,999)".contains(filter) {
        let mut r = urng::wide::Xoshiro128Ppx16::new(1);
        measure("wide::Xoshiro128Ppx16::randi(0,999)", 16 * 64, || { let mut a = 0u64; for _ in 0..64 { a = a.wrapping_add(r.randi(0, 999).iter().fold(0u64, |x, y| x.wrapping_add(*y as u64))); } a });
    }
}

fn cabi(filter: &str) {
    macro_rules! fill {
        ($name:literal, $T:ty, $new:ident($seed:expr), $free:ident, $fill:ident $(, $arg:expr)*) => {
            for &n in &[4096usize, 1 << 20, 64 << 20] {
                let name = format!("cabi::{}[{}]", $name, n);
                if name.contains(filter) {
                    let mut buf: Vec<$T> = vec![Default::default(); n];
                    let h = $new($seed);
                    #[allow(unused_unsafe)]
                    measure(&name, n as u64, || unsafe { $fill(h, buf.as_mut_ptr(), n $(, $arg)*); buf[n / 2] as u64 });
                    #[allow(unused_unsafe)]
                    unsafe { $free(h) };
                }
            }
        };
    }
    fill!("xoshiro128pp_next_u32s", u32, xoshiro128pp_new(1), xoshiro128pp_free, xoshiro128pp_next_u32s);
    fill!("sfc32_next_u32s", u32, sfc32_new(1), sfc32_free, sfc32_next_u32s);
    fill!("mt19937_next_u32s", u32, mt19937_new(1), mt19937_free, mt19937_next_u32s);
    fill!("philox32_next_u32s", u32, philox32_new(1), philox32_free, philox32_next_u32s);
    fill!("xoshiro256pp_next_u64s", u64, xoshiro256pp_new(1), xoshiro256pp_free, xoshiro256pp_next_u64s);
    fill!("sfc32x16_next_u32s", u32, sfc32x16_new(1), sfc32x16_free, sfc32x16_next_u32s);
    fill!("xoshiro128ppx16_next_u32s", u32, xoshiro128ppx16_new(1), xoshiro128ppx16_free, xoshiro128ppx16_next_u32s);
    fill!("philox32x4x4_next_u32s", u32, philox32x4x4_new(1), philox32x4x4_free, philox32x4x4_next_u32s);
    fill!("pcg32x8_next_u32s", u32, pcg32x8_new(1), pcg32x8_free, pcg32x8_next_u32s);
    fill!("splitmix32x16_next_u32s", u32, splitmix32x16_new(1), splitmix32x16_free, splitmix32x16_next_u32s);
    fill!("sfc32x16_rand_i32s(0,999)", i32, sfc32x16_new(1), sfc32x16_free, sfc32x16_rand_i32s, 0, 999);
    fill!("sfc32x16_rand_f32s(0,1)", f32, sfc32x16_new(1), sfc32x16_free, sfc32x16_rand_f32s, 0.0, 1.0);
    fill!("xoshiro256pp_rand_f64s(0,1)", f64, xoshiro256pp_new(1), xoshiro256pp_free, xoshiro256pp_rand_f64s, 0.0, 1.0);
}

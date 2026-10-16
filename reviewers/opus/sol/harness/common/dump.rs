//! Dumps raw generator outputs for comparison with ref/compare_dump.py.
//! Output format (one line per stream): `<name> <seed-hex> <hex words...>`
//! SIMD/wide streams are dumped per lane as `<name>[lane]`.
use std::io::Write;
use urng::*;

const SEEDS32: [u32; 7] = [0, 1, 2, 3, 42, 0x9E37_79B9, u32::MAX];
const SEEDS64: [u64; 9] = [
    0,
    1,
    2,
    3,
    42,
    (1 << 32) | 1,
    0x61C8_8646_80B5_83EB,
    0x9E37_79B9_7F4A_7C15,
    u64::MAX,
];

fn line<W: std::fmt::LowerHex>(out: &mut impl Write, name: &str, seed: u64, v: &[W]) {
    write!(out, "{name} {seed:x}").unwrap();
    for x in v {
        write!(out, " {x:x}").unwrap();
    }
    writeln!(out).unwrap();
}

macro_rules! scalar32 {
    ($out:expr, $n:expr, $($t:ty),+) => {$(
        for &s in &SEEDS32 {
            let mut r = <$t>::new(s);
            let v: Vec<u32> = (0..$n).map(|_| Rng::nextu(&mut r)).collect();
            line($out, stringify!($t), s as u64, &v);
        }
    )+};
}

macro_rules! scalar64 {
    ($out:expr, $n:expr, $($t:ty),+) => {$(
        for &s in &SEEDS64 {
            let mut r = <$t>::new(s);
            let v: Vec<u64> = (0..$n).map(|_| Rng::nextu(&mut r)).collect();
            line($out, stringify!($t), s, &v);
        }
    )+};
}

/// Collect `calls` lane arrays from `f` and print per-lane streams.
fn lanes<T: Copy + std::fmt::LowerHex, const L: usize>(
    out: &mut impl Write,
    name: &str,
    seed: u64,
    calls: usize,
    mut f: impl FnMut() -> [T; L],
) {
    let blocks: Vec<[T; L]> = (0..calls).map(|_| f()).collect();
    for lane in 0..L {
        let v: Vec<T> = blocks.iter().map(|b| b[lane]).collect();
        line(out, &format!("{name}[{lane}]"), seed, &v);
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("output path");
    let mut out = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let o = &mut out;

    scalar32!(o, 1000, SplitMix32, Xoshiro128Pp, Xoshiro128Ss, Xoroshiro64Ss, Jsf32, Sfc32,
        Xorshift32, Xorshift128, Xorwow, Pcg32, Philox32, Threefry32x4, Threefry32x2, Squares32,
        ChaCha8, ChaCha20, Salsa8, Salsa20);
    scalar32!(o, 15000, Mt19937, Sfmt607, Sfmt1279, Sfmt2281, Sfmt4253, Sfmt11213, Sfmt19937,
        Sfmt44497, Sfmt86243, Sfmt132049, Sfmt216091);
    scalar64!(o, 1000, SplitMix64, Xoshiro256Pp, Xoshiro256Ss, Sfc64, Philox64, Philox4x64,
        Xorshift64, Xoroshiro128Pp, Xoroshiro128Ss, TwistedGFSR, Cet64, Cet256, Threefish256,
        Biski64, WyHash64);
    scalar64!(o, 1500, Mt1993764, Sfmt1993764);

    // xoroshiro128 jump / long_jump: 3 outputs, jump, 3 outputs, long_jump, 3 outputs
    for &s in &SEEDS64 {
        let mut a = Xoroshiro128Pp::new(s);
        let mut b = Xoroshiro128Ss::new(s);
        let mut va = vec![];
        let mut vb = vec![];
        for step in 0..3 {
            for _ in 0..3 {
                va.push(a.nextu());
                vb.push(b.nextu());
            }
            if step == 0 {
                a.jump();
                b.jump();
            } else if step == 1 {
                a.long_jump();
                b.long_jump();
            }
        }
        line(o, "Xoroshiro128Pp_jumps", s, &va);
        line(o, "Xoroshiro128Ss_jumps", s, &vb);
    }

    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    simd(o);
    wide(o);
    o.flush().unwrap();
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
fn simd(o: &mut impl Write) {
    let avx2 = std::arch::is_x86_feature_detected!("avx2");
    let f = std::arch::is_x86_feature_detected!("avx512f");
    let dq = std::arch::is_x86_feature_detected!("avx512dq");
    assert!(avx2 && f && dq, "this dump expects an AVX-512F/DQ CPU");
    const C: usize = 300;
    unsafe fn t<V, const L: usize>(v: V) -> [u32; L] {
        unsafe { std::mem::transmute_copy(&v) }
    }
    for &s in &SEEDS32 {
        let mut r = Sfc32x4::new(s);
        lanes(o, "simd::Sfc32x4", s as u64, C, || r.nextu());
        let mut r = unsafe { Sfc32x8::new(s) };
        lanes(o, "simd::Sfc32x8", s as u64, C, || r.nextu());
        let mut r = unsafe { Sfc32x16::new(s) };
        lanes(o, "simd::Sfc32x16", s as u64, C, || r.nextu());
        let mut r = unsafe { Jsf32x8::new(s) };
        lanes(o, "simd::Jsf32x8", s as u64, C, || unsafe { t::<_, 8>(r.nextuv()) });
        let mut r = unsafe { Jsf32x16::new(s) };
        lanes(o, "simd::Jsf32x16", s as u64, C, || unsafe { t::<_, 16>(r.nextuv()) });
        let mut r = unsafe { Xoroshiro64Ssx8::new(s) };
        lanes(o, "simd::Xoroshiro64Ssx8", s as u64, C, || unsafe { t::<_, 8>(r.nextuv()) });
        let mut r = unsafe { Xoroshiro64Ssx16::new(s) };
        lanes(o, "simd::Xoroshiro64Ssx16", s as u64, C, || unsafe { t::<_, 16>(r.nextuv()) });
        let mut r = unsafe { Xoshiro128Ppx16::new(s) };
        lanes(o, "simd::Xoshiro128Ppx16", s as u64, C, || unsafe { t::<_, 16>(r.nextuv()) });
        let mut r = unsafe { Xoshiro128Ssx16::new(s) };
        lanes(o, "simd::Xoshiro128Ssx16", s as u64, C, || unsafe { t::<_, 16>(r.nextuv()) });
        let mut r = unsafe { SplitMix32x16::new(s) };
        lanes(o, "simd::SplitMix32x16", s as u64, C, || unsafe { t::<_, 16>(r.nextuv()) });
        let mut r = unsafe { Pcg32x8::new(s) };
        lanes(o, "simd::Pcg32x8", s as u64, C, || unsafe { r.nextu() });
        let mut r = Xorshift32x8::new(s);
        lanes(o, "simd::Xorshift32x8", s as u64, C, || unsafe { t::<_, 8>(r.nextuv()) });
        let mut r = unsafe { Philox32x4x4::new(s) };
        lanes(o, "simd::Philox32x4x4", s as u64, C, || unsafe { r.nextu() });
        let mut r = unsafe { Squares32x8::new(s) };
        lanes(o, "simd::Squares32x8", s as u64, C, || unsafe { r.nextu() });
    }
    for &s in &SEEDS64 {
        let mut r = Biski64x8::new(s);
        lanes(o, "simd::Biski64x8", s, C, || unsafe { r.nextu() });
        let mut r = Sfc64x8::new(s);
        lanes(o, "simd::Sfc64x8", s, C, || unsafe { r.nextu() });
        let mut r = Cet64x8::new(s);
        lanes(o, "simd::Cet64x8", s, C, || unsafe { r.nextu() });
        let mut r = Cet256x2::new(s);
        lanes(o, "simd::Cet256x2", s, C, || unsafe { r.nextu() });
        let mut r = Xoshiro256Ssx2::new(s);
        lanes(o, "simd::Xoshiro256Ssx2", s, C, || r.nextu());
    }
}

fn wide(o: &mut impl Write) {
    use urng::wide::RngW;
    const C: usize = 300;
    macro_rules! w32 {
        ($($t:ident),+) => {$(
            for &s in &SEEDS32 {
                let mut r = urng::wide::$t::new(s);
                lanes(o, concat!("wide::", stringify!($t)), s as u64, C, || r.nextu());
            }
        )+};
    }
    macro_rules! w64 {
        ($($t:ident),+) => {$(
            for &s in &SEEDS64 {
                let mut r = urng::wide::$t::new(s);
                lanes(o, concat!("wide::", stringify!($t)), s, C, || r.nextu());
            }
        )+};
    }
    w32!(Jsf32x4, Jsf32x8, Jsf32x16, Sfc32x4, Sfc32x8, Sfc32x16, SplitMix32x4, SplitMix32x8,
        SplitMix32x16, Threefry32x2x4, Threefry32x2x8, Threefry32x2x16, Threefry32x4x4,
        Threefry32x4x8, Threefry32x4x16, Xoroshiro64Ssx4, Xoroshiro64Ssx8, Xoroshiro64Ssx16,
        Xorshift32x4, Xorshift32x8, Xorshift32x16, Xorshift128x4, Xorshift128x8, Xorshift128x16,
        Xorwowx4, Xorwowx8, Xorwowx16, Xoshiro128Ppx4, Xoshiro128Ppx8, Xoshiro128Ppx16,
        Xoshiro128Ssx4, Xoshiro128Ssx8, Xoshiro128Ssx16);
    w64!(Pcg32x4, Pcg32x8, Pcg32x16, Squares32x4, Squares32x8);
}

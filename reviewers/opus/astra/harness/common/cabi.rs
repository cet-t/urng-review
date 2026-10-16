//! C ABI verification: every exported family is driven through its extern "C"
//! functions and compared with the Rust API stream of the same type/seed.
//!
//! Checks per family (seeds 0,1,42,u32::MAX / u64 equivalents):
//!  A. one-call fill of N words == flattened Rust stream
//!  B. multi-call fill with chunk sizes [1,3,5,16,17,64,7,...]:
//!     `exact`   – equals the continuous stream (call-boundary independent)
//!     `block`   – equals the stream where every call restarts at a block boundary
//!  C. next_f == to_f01(words); rand_i/rand_f == Rng::randi/randf over the stream
//!  D. edge cases: zero count + null, null handle, misaligned out, min>max,
//!     non-finite float bounds, degenerate/full ranges, results inside range.
use std::fmt::Debug;
use urng::*;

use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
static FAILS: AtomicUsize = AtomicUsize::new(0);
static CHECKS: AtomicUsize = AtomicUsize::new(0);
fn check(ok: bool, what: &str) {
    CHECKS.fetch_add(1, Relaxed);
    if !ok {
        FAILS.fetch_add(1, Relaxed);
        println!("FAIL {what}");
    }
}

/// Rng adapter over a block producer (flattened lane order).
struct Flat<W: Copy, F: FnMut() -> Vec<W>> {
    f: F,
    buf: Vec<W>,
    pos: usize,
}
impl<W: Copy, F: FnMut() -> Vec<W>> Flat<W, F> {
    fn new(f: F) -> Self {
        Self { f, buf: vec![], pos: 0 }
    }
    fn next_word(&mut self) -> W {
        if self.pos == self.buf.len() {
            self.buf = (self.f)();
            self.pos = 0;
        }
        self.pos += 1;
        self.buf[self.pos - 1]
    }
    fn skip_to_block(&mut self) {
        self.pos = self.buf.len();
    }
}
struct R32<F: FnMut() -> Vec<u32>>(Flat<u32, F>);
impl<F: FnMut() -> Vec<u32>> Rng for R32<F> {
    type Word = u32;
    fn nextu(&mut self) -> u32 {
        self.0.next_word()
    }
}
struct R64<F: FnMut() -> Vec<u64>>(Flat<u64, F>);
impl<F: FnMut() -> Vec<u64>> Rng for R64<F> {
    type Word = u64;
    fn nextu(&mut self) -> u64 {
        self.0.next_word()
    }
}

const CHUNKS: [usize; 12] = [1, 3, 5, 16, 17, 64, 7, 2, 31, 33, 100, 1];
const N: usize = 330; // > sum of CHUNKS (280)

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    Exact,
    Block,
    Other,
}

/// Classify how a chunked fill relates to the continuous stream.
fn classify<T: PartialEq + Copy>(chunked: &[T], exact: &[T], block: &[T]) -> Mode {
    if chunked == exact {
        Mode::Exact
    } else if chunked == block {
        Mode::Block
    } else {
        Mode::Other
    }
}

macro_rules! family {
    (
        $tag:literal, $W:ty, $I:ty, $F:ty, $S:ty, $seeds:expr, $blk:expr,
        new = $new:path, free = $free:path,
        nu = $nu:path, nf = $nf:path, ri = $ri:path, rf = $rf:path,
        reference = |$s:ident| $refmk:expr, rng = $RW:ident
    ) => {{
        let mut modes = vec![];
        let mk = |$s: $S| $refmk;
        for &seed in $seeds.iter() {
            let seed: $S = seed as $S;
            let h = $new(seed);
            assert!(!h.is_null(), "{} constructor returned null", $tag);
            // A + B: one call vs chunked
            let mut exact = Flat::new(mk(seed));
            let exp: Vec<$W> = (0..N).map(|_| exact.next_word()).collect();
            let mut one = vec![0 as $W; N];
            unsafe { $nu(h, one.as_mut_ptr(), N) };
            check(one == exp, &format!("{} seed {seed:#x}: one-call fill == Rust stream", $tag));
            unsafe { $free(h) };

            let h = $new(seed);
            let mut chunked = vec![];
            for &c in CHUNKS.iter() {
                let mut tmp = vec![0 as $W; c];
                unsafe { $nu(h, tmp.as_mut_ptr(), c) };
                chunked.extend(tmp);
            }
            unsafe { $free(h) };
            let mut blockwise = Flat::new(mk(seed));
            let mut bexp = vec![];
            for &c in CHUNKS.iter() {
                for _ in 0..c {
                    bexp.push(blockwise.next_word());
                }
                blockwise.skip_to_block();
            }
            let m = classify(&chunked, &exp[..chunked.len()], &bexp);
            check(m != Mode::Other, &format!("{} seed {seed:#x}: chunked fill matches neither exact nor block semantics", $tag));
            modes.push(m);

            // C: floats and ranges over the same stream (single call)
            let h = $new(seed);
            let mut f = vec![0 as $F; N];
            unsafe { $nf(h, f.as_mut_ptr(), N) };
            unsafe { $free(h) };
            let fexp: Vec<$F> = exp.iter().map(|&w| Word::to_f01(w)).collect();
            check(f.iter().zip(&fexp).all(|(a, b)| a.to_bits() == b.to_bits()),
                &format!("{} seed {seed:#x}: next_f == to_f01(stream)", $tag));
            for &(lo, hi) in &[(-5 as $I, 7 as $I), (<$I>::MIN, <$I>::MAX), (0 as $I, 0 as $I),
                               (<$I>::MIN, <$I>::MIN + 2), (-1 as $I, (1 as $I) << ((std::mem::size_of::<$I>() * 8) - 2))] {
                let h = $new(seed);
                let mut v = vec![0 as $I; N];
                unsafe { $ri(h, v.as_mut_ptr(), N, lo, hi) };
                unsafe { $free(h) };
                let mut r = $RW(Flat::new(mk(seed)));
                let vexp: Vec<$I> = (0..N).map(|_| r.randi(lo, hi)).collect();
                check(v == vexp, &format!("{} seed {seed:#x}: rand_i[{lo},{hi}] == Rng::randi(stream)", $tag));
                check(v.iter().all(|x| *x >= lo && *x <= hi), &format!("{} rand_i in range", $tag));
            }
            for &(lo, hi) in &[(-2.0 as $F, 3.0 as $F), (1.0 as $F, (1.0 as $F).next_up()),
                               (-<$F>::MAX, <$F>::MAX), (0.0 as $F, <$F>::MIN_POSITIVE),
                               (1.0e6 as $F, 1.0e6 as $F + 1.0)] {
                let h = $new(seed);
                let mut v = vec![0 as $F; N];
                unsafe { $rf(h, v.as_mut_ptr(), N, lo, hi) };
                unsafe { $free(h) };
                let mut r = $RW(Flat::new(mk(seed)));
                let vexp: Vec<$F> = (0..N).map(|_| r.randf(lo, hi)).collect();
                check(v.iter().zip(&vexp).all(|(a, b)| a.to_bits() == b.to_bits()),
                    &format!("{} seed {seed:#x}: rand_f[{lo:e},{hi:e}) == Rng::randf(stream)", $tag));
                check(v.iter().all(|x| *x >= lo && *x < hi), &format!("{} seed {seed:#x}: rand_f[{lo:e},{hi:e}) half-open", $tag));
            }
        }
        // D: edge cases (single seed)
        let h = $new(1 as $S);
        unsafe {
            $nu(h, std::ptr::null_mut(), 0);
            $nu(h, std::ptr::null_mut(), 10);
            $nu(std::ptr::null_mut(), [0 as $W; 4].as_mut_ptr(), 4);
            let mut big = vec![0x5Au8; 64];
            let mis = big.as_mut_ptr().add(1) as *mut $W;
            $nu(h, mis, 4);
            check(big.iter().all(|&b| b == 0x5A), &format!("{}: misaligned output must not be written", $tag));
            let mut v = vec![<$I>::MAX; 8];
            $ri(h, v.as_mut_ptr(), 8, 5 as $I, 4 as $I);
            check(v.iter().all(|x| *x == <$I>::MAX), &format!("{}: rand_i min>max must be a no-op", $tag));
            for &(lo, hi) in &[(<$F>::NAN, 1.0 as $F), (0.0 as $F, <$F>::INFINITY), (1.0 as $F, 1.0 as $F), (2.0 as $F, 1.0 as $F)] {
                let mut v = vec![-7.0 as $F; 8];
                $rf(h, v.as_mut_ptr(), 8, lo, hi);
                check(v.iter().all(|x| *x == -7.0 as $F), &format!("{}: rand_f({lo},{hi}) must be a no-op", $tag));
            }
            // state must be untouched by all the rejected calls above
            let mut after = vec![0 as $W; 16];
            $nu(h, after.as_mut_ptr(), 16);
            let mut fresh = Flat::new(mk(1 as $S));
            let exp16: Vec<$W> = (0..16).map(|_| fresh.next_word()).collect();
            check(after == exp16, &format!("{}: rejected calls must not advance the stream", $tag));
            $free(h);
            $free(std::ptr::null_mut());
        }
        let first = modes[0];
        let uniform = modes.iter().all(|m| *m == first);
        println!("family {:<16} chunked-fill semantics: {:?}{}", $tag, first, if uniform { "" } else { " (varies by seed!)" });
    }};
}

fn v32<const L: usize>(a: [u32; L]) -> Vec<u32> {
    a.to_vec()
}
fn v64<const L: usize>(a: [u64; L]) -> Vec<u64> {
    a.to_vec()
}
#[cfg(target_arch = "x86_64")]
unsafe fn t32<V, const L: usize>(v: V) -> Vec<u32> {
    let a: [u32; L] = unsafe { std::mem::transmute_copy(&v) };
    a.to_vec()
}

macro_rules! scalar_family32 {
    ($tag:literal, $T:ty, $p:ident, $S:ty) => {
        pastey::paste! {
            family!($tag, u32, i32, f32, $S, [0u64, 1, 42, u32::MAX as u64], 1,
                new = [<$p _new>], free = [<$p _free>],
                nu = [<$p _next_u32s>], nf = [<$p _next_f32s>], ri = [<$p _rand_i32s>], rf = [<$p _rand_f32s>],
                reference = |s| { let mut r = <$T>::new(s as _); move || vec![Rng::nextu(&mut r)] }, rng = R32)
        }
    };
}
macro_rules! scalar_family64 {
    ($tag:literal, $T:ty, $p:ident) => {
        pastey::paste! {
            family!($tag, u64, i64, f64, u64, [0u64, 1, 42, u64::MAX], 1,
                new = [<$p _new>], free = [<$p _free>],
                nu = [<$p _next_u64s>], nf = [<$p _next_f64s>], ri = [<$p _rand_i64s>], rf = [<$p _rand_f64s>],
                reference = |s| { let mut r = <$T>::new(s); move || vec![Rng::nextu(&mut r)] }, rng = R64)
        }
    };
}

fn main() {
    scalar_family32!("splitmix32", SplitMix32, splitmix32, u32);
    scalar_family32!("xoshiro128pp", Xoshiro128Pp, xoshiro128pp, u32);
    scalar_family32!("xoshiro128ss", Xoshiro128Ss, xoshiro128ss, u32);
    scalar_family32!("xoroshiro64ss", Xoroshiro64Ss, xoroshiro64ss, u32);
    scalar_family32!("jsf32", Jsf32, jsf32, u32);
    scalar_family32!("sfc32", Sfc32, sfc32, u32);
    scalar_family32!("xorshift32", Xorshift32, xorshift32, u32);
    scalar_family32!("xorshift128", Xorshift128, xorshift128, u32);
    scalar_family32!("pcg32", Pcg32, pcg32, u32);
    scalar_family32!("philox32", Philox32, philox32, u32);
    scalar_family32!("threefry32x4", Threefry32x4, threefry32x4, u32);
    scalar_family32!("squares32", Squares32, squares32, u32);
    scalar_family32!("mt19937", Mt19937, mt19937, u32);
    scalar_family32!("sfmt607", Sfmt607, sfmt607, u64);
    scalar_family32!("sfmt1279", Sfmt1279, sfmt1279, u64);
    scalar_family32!("sfmt2281", Sfmt2281, sfmt2281, u64);
    scalar_family32!("sfmt4253", Sfmt4253, sfmt4253, u64);
    scalar_family32!("sfmt11213", Sfmt11213, sfmt11213, u64);
    scalar_family32!("sfmt19937", Sfmt19937, sfmt19937, u64);
    scalar_family32!("sfmt44497", Sfmt44497, sfmt44497, u64);
    scalar_family32!("sfmt86243", Sfmt86243, sfmt86243, u64);
    scalar_family32!("sfmt132049", Sfmt132049, sfmt132049, u64);
    scalar_family32!("sfmt216091", Sfmt216091, sfmt216091, u64);
    #[cfg(feature = "v_sol")]
    scalar_family32!("threefry32x2", Threefry32x2, threefry32x2, u32);

    scalar_family64!("splitmix64", SplitMix64, splitmix64);
    scalar_family64!("xoshiro256pp", Xoshiro256Pp, xoshiro256pp);
    scalar_family64!("xoshiro256ss", Xoshiro256Ss, xoshiro256ss);
    scalar_family64!("xoroshiro128pp", Xoroshiro128Pp, xoroshiro128pp);
    scalar_family64!("xoroshiro128ss", Xoroshiro128Ss, xoroshiro128ss);
    scalar_family64!("xorshift64", Xorshift64, xorshift64);
    scalar_family64!("sfc64", Sfc64, sfc64);
    scalar_family64!("philox64", Philox64, philox64);
    scalar_family64!("mt1993764", Mt1993764, mt1993764);
    scalar_family64!("twisted_gfsr", TwistedGFSR, twisted_gfsr);
    scalar_family64!("cet64", Cet64, cet64);
    scalar_family64!("cet256", Cet256, cet256);
    scalar_family64!("threefish256", Threefish256, threefish256);
    scalar_family64!("biski64", Biski64, biski64);
    #[cfg(feature = "v_sol")]
    scalar_family64!("sfmt1993764", Sfmt1993764, sfmt1993764);
    // Base-compatible surface (Astra): rand_f64s is exported as `sfmt_rand_f64s`
    #[cfg(feature = "v_astra")]
    family!("sfmt1993764", u64, i64, f64, u64, [0u64, 1, 42, u64::MAX], 1,
        new = sfmt1993764_new, free = sfmt1993764_free, nu = sfmt1993764_next_u64s,
        nf = sfmt1993764_next_f64s, ri = sfmt1993764_rand_i64s, rf = sfmt_rand_f64s,
        reference = |s| { let mut r = Sfmt1993764::new(s); move || vec![Rng::nextu(&mut r)] }, rng = R64);
    // Astra exports no threefry32x2_rand_f32s: check the other three fills
    #[cfg(feature = "v_astra")]
    for seed in [0u32, 1, 42, u32::MAX] {
        let mut r = Threefry32x2::new(seed);
        let exp: Vec<u32> = (0..N).map(|_| r.nextu()).collect();
        let h = threefry32x2_new(seed);
        let mut chunked = vec![];
        for &c in CHUNKS.iter() {
            let mut tmp = vec![0u32; c];
            unsafe { threefry32x2_next_u32s(h, tmp.as_mut_ptr(), c) };
            chunked.extend(tmp);
        }
        unsafe { threefry32x2_free(h) };
        check(chunked[..] == exp[..chunked.len()], "threefry32x2 chunked == stream");
        let h = threefry32x2_new(seed);
        let mut f = vec![0f32; N];
        unsafe { threefry32x2_next_f32s(h, f.as_mut_ptr(), N) };
        unsafe { threefry32x2_free(h) };
        check(f.iter().zip(&exp).all(|(a, w)| a.to_bits() == Word::to_f01(*w).to_bits()), "threefry32x2 next_f32s");
        let h = threefry32x2_new(seed);
        let mut v = vec![0i32; N];
        unsafe { threefry32x2_rand_i32s(h, v.as_mut_ptr(), N, -5, 7) };
        unsafe { threefry32x2_free(h) };
        let mut r = Threefry32x2::new(seed);
        check(v.iter().all(|x| *x == r.randi(-5, 7)), "threefry32x2 rand_i32s");
    }

    #[cfg(target_arch = "x86_64")]
    simd_families();

    println!("checks={} fails={}", CHECKS.load(Relaxed), FAILS.load(Relaxed));
    if FAILS.load(Relaxed) > 0 {
        std::process::exit(1);
    }
}

#[cfg(target_arch = "x86_64")]
fn simd_families() {
    let s32 = [0u64, 1, 42, u32::MAX as u64];
    let s64 = [0u64, 1, 42, u64::MAX];
    family!("sfc32x4", u32, i32, f32, u32, s32, 4, new = sfc32x4_new, free = sfc32x4_free,
        nu = sfc32x4_next_u32s, nf = sfc32x4_next_f32s, ri = sfc32x4_rand_i32s, rf = sfc32x4_rand_f32s,
        reference = |s| { let mut r = Sfc32x4::new(s); move || v32(r.nextu()) }, rng = R32);
    family!("sfc32x8", u32, i32, f32, u32, s32, 8, new = sfc32x8_new, free = sfc32x8_free,
        nu = sfc32x8_next_u32s, nf = sfc32x8_next_f32s, ri = sfc32x8_rand_i32s, rf = sfc32x8_rand_f32s,
        reference = |s| { let mut r = unsafe { Sfc32x8::new(s) }; move || v32(r.nextu()) }, rng = R32);
    family!("sfc32x16", u32, i32, f32, u32, s32, 16, new = sfc32x16_new, free = sfc32x16_free,
        nu = sfc32x16_next_u32s, nf = sfc32x16_next_f32s, ri = sfc32x16_rand_i32s, rf = sfc32x16_rand_f32s,
        reference = |s| { let mut r = unsafe { Sfc32x16::new(s) }; move || v32(r.nextu()) }, rng = R32);
    family!("jsf32x16", u32, i32, f32, u32, s32, 16, new = jsf32x16_new, free = jsf32x16_free,
        nu = jsf32x16_next_u32s, nf = jsf32x16_next_f32s, ri = jsf32x16_rand_i32s, rf = jsf32x16_rand_f32s,
        reference = |s| { let mut r = unsafe { Jsf32x16::new(s) }; move || unsafe { t32::<_, 16>(r.nextuv()) } }, rng = R32);
    family!("xoroshiro64ssx8", u32, i32, f32, u32, s32, 8, new = xoroshiro64ssx8_new, free = xoroshiro64ssx8_free,
        nu = xoroshiro64ssx8_next_u32s, nf = xoroshiro64ssx8_next_f32s, ri = xoroshiro64ssx8_rand_i32s, rf = xoroshiro64ssx8_rand_f32s,
        reference = |s| { let mut r = unsafe { Xoroshiro64Ssx8::new(s) }; move || unsafe { t32::<_, 8>(r.nextuv()) } }, rng = R32);
    family!("xoroshiro64ssx16", u32, i32, f32, u32, s32, 16, new = xoroshiro64ssx16_new, free = xoroshiro64ssx16_free,
        nu = xoroshiro64ssx16_next_u32s, nf = xoroshiro64ssx16_next_f32s, ri = xoroshiro64ssx16_rand_i32s, rf = xoroshiro64ssx16_rand_f32s,
        reference = |s| { let mut r = unsafe { Xoroshiro64Ssx16::new(s) }; move || unsafe { t32::<_, 16>(r.nextuv()) } }, rng = R32);
    family!("xoshiro128ppx16", u32, i32, f32, u32, s32, 16, new = xoshiro128ppx16_new, free = xoshiro128ppx16_free,
        nu = xoshiro128ppx16_next_u32s, nf = xoshiro128ppx16_next_f32s, ri = xoshiro128ppx16_rand_i32s, rf = xoshiro128ppx16_rand_f32s,
        reference = |s| { let mut r = unsafe { Xoshiro128Ppx16::new(s) }; move || unsafe { t32::<_, 16>(r.nextuv()) } }, rng = R32);
    family!("xoshiro128ssx16", u32, i32, f32, u32, s32, 16, new = xoshiro128ssx16_new, free = xoshiro128ssx16_free,
        nu = xoshiro128ssx16_next_u32s, nf = xoshiro128ssx16_next_f32s, ri = xoshiro128ssx16_rand_i32s, rf = xoshiro128ssx16_rand_f32s,
        reference = |s| { let mut r = unsafe { Xoshiro128Ssx16::new(s) }; move || unsafe { t32::<_, 16>(r.nextuv()) } }, rng = R32);
    family!("pcg32x8", u32, i32, f32, u32, s32, 8, new = pcg32x8_new, free = pcg32x8_free,
        nu = pcg32x8_next_u32s, nf = pcg32x8_next_f32s, ri = pcg32x8_rand_i32s, rf = pcg32x8_rand_f32s,
        reference = |s| { let mut r = unsafe { Pcg32x8::new(s) }; move || v32(unsafe { r.nextu() }) }, rng = R32);
    family!("philox32x4x4", u32, i32, f32, u32, s32, 16, new = philox32x4x4_new, free = philox32x4x4_free,
        nu = philox32x4x4_next_u32s, nf = philox32x4x4_next_f32s, ri = philox32x4x4_rand_i32s, rf = philox32x4x4_rand_f32s,
        reference = |s| { let mut r = unsafe { Philox32x4x4::new(s) }; move || v32(unsafe { r.nextu() }) }, rng = R32);
    family!("squares32x8", u32, i32, f32, u32, s32, 8, new = squares32x8_new, free = squares32x8_free,
        nu = squares32x8_next_u32s, nf = squares32x8_next_f32s, ri = squares32x8_rand_i32s, rf = squares32x8_rand_f32s,
        reference = |s| { let mut r = unsafe { Squares32x8::new(s) }; move || v32(unsafe { r.nextu() }) }, rng = R32);
    family!("biski64x8", u64, i64, f64, u64, s64, 8, new = biski64x8_new, free = biski64x8_free,
        nu = biski64x8_next_u64s, nf = biski64x8_next_f64s, ri = biski64x8_rand_i64s, rf = biski64x8_rand_f64s,
        reference = |s| { let mut r = Biski64x8::new(s); move || v64(unsafe { r.nextu() }) }, rng = R64);
    #[cfg(feature = "v_sol")]
    {
        family!("splitmix32x16", u32, i32, f32, u32, s32, 16, new = splitmix32x16_new, free = splitmix32x16_free,
            nu = splitmix32x16_next_u32s, nf = splitmix32x16_next_f32s, ri = splitmix32x16_rand_i32s, rf = splitmix32x16_rand_f32s,
            reference = |s| { let mut r = unsafe { SplitMix32x16::new(s) }; move || unsafe { t32::<_, 16>(r.nextuv()) } }, rng = R32);
        family!("sfc64x8", u64, i64, f64, u64, s64, 8, new = sfc64x8_new, free = sfc64x8_free,
            nu = sfc64x8_next_u64s, nf = sfc64x8_next_f64s, ri = sfc64x8_rand_i64s, rf = sfc64x8_rand_f64s,
            reference = |s| { let mut r = Sfc64x8::new(s); move || v64(unsafe { r.nextu() }) }, rng = R64);
        family!("cet64x8", u64, i64, f64, u64, s64, 8, new = cet64x8_new, free = cet64x8_free,
            nu = cet64x8_next_u64s, nf = cet64x8_next_f64s, ri = cet64x8_rand_i64s, rf = cet64x8_rand_f64s,
            reference = |s| { let mut r = Cet64x8::new(s); move || v64(unsafe { r.nextu() }) }, rng = R64);
        family!("cet256x2", u64, i64, f64, u64, s64, 2, new = cet256x2_new, free = cet256x2_free,
            nu = cet256x2_next_u64s, nf = cet256x2_next_f64s, ri = cet256x2_rand_i64s, rf = cet256x2_rand_f64s,
            reference = |s| { let mut r = Cet256x2::new(s); move || v64(unsafe { r.nextu() }) }, rng = R64);
        family!("xoshiro256ssx2", u64, i64, f64, u64, s64, 2, new = xoshiro256ssx2_new, free = xoshiro256ssx2_free,
            nu = xoshiro256ssx2_next_u64s, nf = xoshiro256ssx2_next_f64s, ri = xoshiro256ssx2_rand_i64s, rf = xoshiro256ssx2_rand_f64s,
            reference = |s| { let mut r = Xoshiro256Ssx2::new(s); move || v64(r.nextu()) }, rng = R64);
    }
    // Families that only export new/free/next_u in the base-compatible surface
    // are checked for stream equivalence only.
    #[cfg(feature = "v_astra")]
    {
        only_next_u_checks();
    }
}

#[cfg(all(target_arch = "x86_64", feature = "v_astra"))]
fn only_next_u_checks() {
    macro_rules! nu_only {
        ($tag:literal, $W:ty, $S:ty, $seeds:expr, $new:path, $free:path, $nu:path, |$s:ident| $mk:expr) => {
            for &seed in $seeds.iter() {
                let seed = seed as $S;
                let mk = |$s: $S| $mk;
                let mut f = Flat::new(mk(seed));
                let exp: Vec<$W> = (0..N).map(|_| f.next_word()).collect();
                let h = $new(seed);
                let mut chunked = vec![];
                for &c in CHUNKS.iter() {
                    let mut tmp = vec![0 as $W; c];
                    unsafe { $nu(h, tmp.as_mut_ptr(), c) };
                    chunked.extend(tmp);
                }
                unsafe { $free(h) };
                check(chunked[..] == exp[..chunked.len()], &format!("{} seed {seed:#x}: chunked next_u == continuous stream", $tag));
            }
            println!("family {:<16} next_u only (no f/rand exports)", $tag);
        };
    }
    let s32 = [0u64, 1, 42, u32::MAX as u64];
    let s64 = [0u64, 1, 42, u64::MAX];
    nu_only!("splitmix32x16", u32, u32, s32, splitmix32x16_new, splitmix32x16_free, splitmix32x16_next_u32s,
        |s| { let mut r = unsafe { SplitMix32x16::new(s) }; move || unsafe { t32::<_, 16>(r.nextuv()) } });
    nu_only!("sfc64x8", u64, u64, s64, sfc64x8_new, sfc64x8_free, sfc64x8_next_u64s,
        |s| { let mut r = Sfc64x8::new(s); move || v64(unsafe { r.nextu() }) });
    nu_only!("cet64x8", u64, u64, s64, cet64x8_new, cet64x8_free, cet64x8_next_u64s,
        |s| { let mut r = Cet64x8::new(s); move || v64(unsafe { r.nextu() }) });
    nu_only!("cet256x2", u64, u64, s64, cet256x2_new, cet256x2_free, cet256x2_next_u64s,
        |s| { let mut r = Cet256x2::new(s); move || v64(unsafe { r.nextu() }) });
    nu_only!("xoshiro256ssx2", u64, u64, s64, xoshiro256ssx2_new, xoshiro256ssx2_free, xoshiro256ssx2_next_u64s,
        |s| { let mut r = Xoshiro256Ssx2::new(s); move || v64(r.nextu()) });
}

#[allow(dead_code)]
fn _unused<T: Debug>(_: T) {}

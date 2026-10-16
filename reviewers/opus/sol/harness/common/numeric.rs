//! Numeric conversion / sampling probes.
//! usage: numeric <probe>
//!   randi_enum   exhaustive 2^32-word check of Rng::randi rejection sampling
//!   randi64_edge boundary words for the 64-bit rejection threshold
//!   randf_edge   adversarial words / ranges for the [min,max) guarantee
//!   index        Rng::index bounds incl. len > 2^32 with a 32-bit word
//!   shuffle      chi-square of permutation frequencies
//!   sampler      zero / extreme / invalid weights, distribution accuracy
//!   simdconv     WordV / RngW conversions (randiv / randfv / to_randi)
//!   randcore     rand_core TryRng word order / fill_bytes / SeedableRng
use std::panic::{AssertUnwindSafe, catch_unwind};
use urng::*;

/// Returns `first`, then words from a fallback sequence; counts draws.
struct OneShot32 {
    first: u32,
    draws: u32,
    fallback: u32,
}
impl Rng for OneShot32 {
    type Word = u32;
    fn nextu(&mut self) -> u32 {
        self.draws += 1;
        if self.draws == 1 {
            self.first
        } else {
            self.fallback = self.fallback.wrapping_mul(0x9E37_79B9).wrapping_add(0x7F4A_7C15);
            self.fallback
        }
    }
}
struct Fixed32(Vec<u32>, usize);
impl Rng for Fixed32 {
    type Word = u32;
    fn nextu(&mut self) -> u32 {
        let v = self.0[self.1 % self.0.len()];
        self.1 += 1;
        v
    }
}
struct Fixed64(Vec<u64>, usize);
impl Rng for Fixed64 {
    type Word = u64;
    fn nextu(&mut self) -> u64 {
        let v = self.0[self.1 % self.0.len()];
        self.1 += 1;
        v
    }
}

fn randi_enum() {
    let ranges: [(i32, i32); 7] = [
        (0, 2),
        (-5, 7),
        (0, 1 << 30),
        (0, i32::MAX),          // 2^31 values
        (-1, i32::MAX),         // 2^31 + 1 values
        (i32::MIN, i32::MAX - 1),
        (i32::MIN, i32::MAX),   // full range
    ];
    for &(lo, hi) in &ranges {
        let range = (hi as i64 - lo as i64 + 1) as u64;
        let threads = 16u64;
        // bucket counts would be huge for big ranges; instead check the exact
        // invariant: every accepted first word w yields (w*range)>>32, and the
        // number of first-word rejections equals 2^32 mod range; buckets are
        // equally populated iff accepted words per bucket == floor(2^32/range).
        // For small ranges we additionally count buckets explicitly.
        let small = range <= 64;
        let results: Vec<(u64, Vec<u64>, bool)> = std::thread::scope(|s| {
            let hs: Vec<_> = (0..threads)
                .map(|t| {
                    s.spawn(move || {
                        let mut rejected = 0u64;
                        let mut buckets = vec![0u64; if small { range as usize } else { 0 }];
                        let mut mapping_ok = true;
                        let lo_w = (t << 32) / threads;
                        let hi_w = ((t + 1) << 32) / threads;
                        for w in lo_w..hi_w {
                            let mut r = OneShot32 { first: w as u32, draws: 0, fallback: w as u32 };
                            let v = r.randi(lo, hi);
                            if r.draws > 1 {
                                rejected += 1;
                            } else {
                                let expect = (((w * range) >> 32) as i64 + lo as i64) as i32;
                                if v != expect {
                                    mapping_ok = false;
                                }
                                if small {
                                    buckets[(v as i64 - lo as i64) as usize] += 1;
                                }
                            }
                            if v < lo || v > hi {
                                mapping_ok = false;
                            }
                        }
                        (rejected, buckets, mapping_ok)
                    })
                })
                .collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let rejected: u64 = results.iter().map(|r| r.0).sum();
        let mapping_ok = results.iter().all(|r| r.2);
        let expected_rej = if range == 1 << 32 { 0 } else { (1u64 << 32) % range };
        let mut bucket_line = String::new();
        if small {
            let mut b = vec![0u64; range as usize];
            for r in &results {
                for (i, c) in r.1.iter().enumerate() {
                    b[i] += c;
                }
            }
            let per = (1u64 << 32) / range;
            let equal = b.iter().all(|&c| c == per);
            bucket_line = format!(" buckets all == floor(2^32/range)={per}: {equal}");
        }
        println!(
            "randi[{lo},{hi}] range={range}: first-word rejections={rejected} (exact expectation {expected_rej}) {} mapping/in-range ok={mapping_ok}{bucket_line}",
            if rejected == expected_rej { "OK" } else { "MISMATCH" }
        );
    }
}

fn randi64_edge() {
    // For range r, threshold t = 2^64 mod r. A first word w is rejected iff
    // (w*r mod 2^64) < t. Construct words just below/at the threshold.
    for &(lo, hi) in &[(0i64, 2), (-5, 7), (0, (1i64 << 62) + 12345), (i64::MIN, i64::MAX - 1), (i64::MIN + 7, i64::MAX)] {
        let r = (hi as i128 - lo as i128 + 1) as u128;
        let t = (1u128 << 64) % r;
        // Find words w with low64(w*r) == t-1 (must reject) and == t (accept):
        // w = target * r^{-1} mod 2^64 works when r is odd.
        let mut tested = 0;
        let mut ok = true;
        if r % 2 == 1 && t > 0 {
            let inv = modinv_u64(r as u64);
            for target in [t - 1, t, 0u128, (t + 1) % (1 << 64)] {
                let w = ((target as u64) as u128 * inv as u128) as u64;
                let mut g = Fixed64(vec![w, 1u64 << 63], 0);
                let v = g.randi(lo, hi);
                let rejected = g.1 > 1;
                let should_reject = ((w as u128 * r) as u64 as u128) < t;
                tested += 1;
                if rejected != should_reject || v < lo || v > hi {
                    ok = false;
                    println!("  w={w:#x} rejected={rejected} expected={should_reject} v={v}");
                }
            }
        }
        // random words: result always in range, mapping equals multiply-high
        let mut x = 0x1234_5678_9ABC_DEF0u64;
        for _ in 0..200_000 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let mut g = Fixed64(vec![x, 1u64 << 63, 0, u64::MAX], 0);
            let v = g.randi(lo, hi);
            if v < lo || v > hi {
                ok = false;
            }
            if g.1 == 1 {
                let expect = (((x as u128 * r) >> 64) as i128 + lo as i128) as i64;
                if v != expect {
                    ok = false;
                }
            }
        }
        println!("randi64[{lo},{hi}] threshold-boundary words tested={tested} ok={ok}");
    }
}

fn modinv_u64(a: u64) -> u64 {
    // Newton iteration for inverse modulo 2^64 (a odd)
    let mut x = a;
    for _ in 0..6 {
        x = x.wrapping_mul(2u64.wrapping_sub(a.wrapping_mul(x)));
    }
    x
}

fn randf_edge() {
    let words32 = [0u32, 1, 0x1FF, 0x200, 0x7FFF_FFFF, 0x8000_0000, 0xFFFF_FE00, 0xFFFF_FFFF];
    let ranges32: [(f32, f32); 12] = [
        (0.0, 1.0),
        (-1.0, 1.0),
        (1.0, 1.0f32.next_up()),
        (16_777_216.0, 16_777_218.0),
        (1.0e7, 1.0e7 + 1.0),
        (-f32::MAX, f32::MAX),
        (f32::MAX / 2.0, f32::MAX),
        (-f32::MAX, -f32::MAX / 2.0),
        (0.0, f32::MIN_POSITIVE),
        (-1.0e-45, 1.0e-45),
        (-3.0, -2.0),
        (0.1, 0.3),
    ];
    let mut bad = 0;
    let mut total = 0;
    for &(lo, hi) in &ranges32 {
        for &w in &words32 {
            let mut g = Fixed32(vec![w], 0);
            let v = g.randf(lo, hi);
            total += 1;
            if !(v >= lo && v < hi) {
                bad += 1;
                println!("  f32 randf({lo:e},{hi:e}) word={w:#x} -> {v:e} OUTSIDE [min,max)");
            }
        }
    }
    let words64 = [0u64, 1, 0x7FF, 0x800, u64::MAX >> 1, 1 << 63, u64::MAX - 0x7FF, u64::MAX];
    let ranges64: [(f64, f64); 8] = [
        (0.0, 1.0),
        (1.0, 1.0f64.next_up()),
        (9_007_199_254_740_992.0, 9_007_199_254_740_996.0),
        (-f64::MAX, f64::MAX),
        (f64::MAX / 2.0, f64::MAX),
        (0.0, f64::MIN_POSITIVE),
        (-5e-324, 5e-324),
        (0.1, 0.3),
    ];
    for &(lo, hi) in &ranges64 {
        for &w in &words64 {
            let mut g = Fixed64(vec![w], 0);
            let v = g.randf(lo, hi);
            total += 1;
            if !(v >= lo && v < hi) {
                bad += 1;
                println!("  f64 randf({lo:e},{hi:e}) word={w:#x} -> {v:e} OUTSIDE [min,max)");
            }
        }
    }
    println!("randf edge: {total} cases, {bad} outside [min,max)");
    // invalid inputs: expected to panic (documented in both variants)
    for &(lo, hi) in &[(1.0f32, 1.0f32), (2.0, 1.0), (f32::NAN, 1.0), (0.0, f32::INFINITY)] {
        let r = catch_unwind(AssertUnwindSafe(|| {
            let mut g = Fixed32(vec![123], 0);
            g.randf(lo, hi)
        }));
        println!("  randf({lo},{hi}) -> {}", match r { Ok(v) => format!("returned {v}"), Err(_) => "panic".into() });
    }
    // randi with min > max
    let r = catch_unwind(AssertUnwindSafe(|| {
        let mut g = Fixed32(vec![123], 0);
        g.randi(5, 4)
    }));
    println!("  randi(5,4) -> {}", match r { Ok(v) => format!("returned {v}"), Err(_) => "panic".into() });
}

fn index() {
    let lens: [usize; 9] = [1, 2, 3, 1000, u32::MAX as usize, 1 << 32, (1 << 32) + 1, 1 << 63, usize::MAX];
    let mut rng = Xoshiro128Pp::new(7);
    let mut rng64 = Xoshiro256Pp::new(7);
    for &len in &lens {
        let mut ok = true;
        let mut max_seen = 0usize;
        for _ in 0..200_000 {
            let a = rng.index(len);
            let b = rng64.index(len);
            if a >= len || b >= len {
                ok = false;
            }
            max_seen = max_seen.max(a);
        }
        println!("index({len:#x}) 32-bit+64-bit word: in range={ok}; 32-bit max seen={max_seen:#x}");
    }
    // len > 2^32 with a 32-bit RNG must be able to reach indices >= 2^32
    let mut hit_high = false;
    for _ in 0..1000 {
        if rng.index(1usize << 40) >= (1usize << 32) {
            hit_high = true;
            break;
        }
    }
    println!("index(2^40) with 32-bit word reaches >= 2^32: {hit_high}");
    // exhaustive exactness of index(3) via one-shot words (same scheme as randi)
    let mut buckets = [0u64; 3];
    let mut rejected = 0u64;
    for w in 0..=u32::MAX {
        let mut r = OneShot32 { first: w, draws: 0, fallback: w };
        let i = r.index(3);
        if r.draws > 1 {
            rejected += 1;
        } else {
            buckets[i] += 1;
        }
    }
    println!("index(3) exhaustive: buckets={buckets:?} rejected={rejected} (expect each {} and {} rejected)", (1u64 << 32) / 3, (1u64 << 32) % 3);
}

fn shuffle() {
    let mut rng = Sfc64::new(99);
    let n = 5usize;
    let trials = 1_200_000usize;
    let mut counts = std::collections::HashMap::new();
    for _ in 0..trials {
        let mut v: Vec<u8> = (0..n as u8).collect();
        rng.shuffle(&mut v).unwrap();
        *counts.entry(v).or_insert(0usize) += 1;
    }
    let k = counts.len();
    let e = trials as f64 / 120.0;
    let chi: f64 = counts.values().map(|&c| (c as f64 - e).powi(2) / e).sum::<f64>()
        + (120 - k) as f64 * e;
    // df=119: mean 119, sd ~15.4; 99.9% quantile ~ 169
    println!("shuffle n=5: {k}/120 permutations seen, chi2={chi:.1} (df=119, 99.9% crit ~169)");
    // position-of-element-0 distribution via shuffled()
    let mut pos = [0usize; 5];
    for _ in 0..500_000 {
        let it: Vec<&u8> = rng.shuffled(&[0u8, 1, 2, 3, 4]).unwrap().collect();
        let p = it.iter().position(|&&x| x == 0).unwrap();
        pos[p] += 1;
    }
    println!("shuffled(): position of element 0 counts {pos:?} (expect ~100000 each)");
    let empty: &mut [u8] = &mut [];
    println!("shuffle(empty) -> {:?}", rng.shuffle(empty).is_err());
}

fn sampler() {
    fn freq<S: FnMut() -> usize>(mut f: S, n: usize, trials: usize) -> Vec<f64> {
        let mut c = vec![0usize; n];
        for _ in 0..trials {
            c[f()] += 1;
        }
        c.iter().map(|&x| x as f64 / trials as f64).collect()
    }
    let cases32: Vec<Vec<f32>> = vec![
        vec![0.0, 1.0],
        vec![1.0, 0.0],
        vec![0.0, 0.0, 1.0, 0.0],
        vec![1e-30, 0.0, 1.0],
        vec![f32::MAX, f32::MAX],
        vec![f32::MAX, 0.0, f32::MAX],
        vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0],
        vec![0.1, 0.2, 0.3, 0.15, 0.25],
        vec![3.0, 3.0, 3.0],
    ];
    for w in &cases32 {
        let total: f64 = w.iter().map(|&x| x as f64).sum();
        let exp: Vec<f64> = w.iter().map(|&x| x as f64 / total).collect();
        for kind in ["Alias32", "Bst32"] {
            let r = catch_unwind(AssertUnwindSafe(|| {
                let mut rng = Sfc32::new(5);
                if kind == "Alias32" {
                    let mut s = Alias32::new(&mut rng, w);
                    freq(|| s.sample(), w.len(), 400_000)
                } else {
                    let mut s = Bst32::new(&mut rng, w);
                    freq(|| s.sample(), w.len(), 400_000)
                }
            }));
            match r {
                Ok(f) => {
                    let maxdev = f.iter().zip(&exp).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
                    let zero_hit = w.iter().zip(&f).any(|(&wi, &fi)| wi == 0.0 && fi > 0.0);
                    println!("{kind:<7} {w:?}: freq={:?} max|dev|={maxdev:.4}{}{}",
                        f.iter().map(|x| (x * 1e4).round() / 1e4).collect::<Vec<_>>(),
                        if maxdev > 0.01 { "  <-- WRONG DISTRIBUTION" } else { "" },
                        if zero_hit { "  <-- ZERO-WEIGHT ENTRY SAMPLED" } else { "" });
                }
                Err(_) => println!("{kind:<7} {w:?}: panic"),
            }
        }
    }
    // invalid weights
    for w in [vec![], vec![0.0f32, 0.0], vec![-1.0, 2.0], vec![f32::NAN, 1.0], vec![f32::INFINITY, 1.0]] {
        for kind in ["Alias32", "Bst32"] {
            let r = catch_unwind(AssertUnwindSafe(|| {
                let mut rng = Sfc32::new(5);
                if kind == "Alias32" {
                    let mut s = Alias32::new(&mut rng, &w);
                    (0..1000).map(|_| s.sample()).collect::<Vec<_>>()
                } else {
                    let mut s = Bst32::new(&mut rng, &w);
                    (0..1000).map(|_| s.sample()).collect::<Vec<_>>()
                }
            }));
            match r {
                Ok(v) => {
                    let mut h = vec![0usize; w.len().max(1)];
                    let last = h.len() - 1;
                    for i in v {
                        h[i.min(last)] += 1;
                    }
                    println!("{kind:<7} invalid {w:?}: no panic, returned histogram {h:?}");
                }
                Err(_) => println!("{kind:<7} invalid {w:?}: panic"),
            }
        }
    }
    // Randomised accuracy sweep for Alias32/Alias64 (detects lost columns)
    let mut lcg = 0x1234_5678u64;
    let mut worst32 = (0.0f64, vec![]);
    let mut worst64 = (0.0f64, vec![]);
    let mut flagged32 = 0;
    let mut flagged64 = 0;
    for case in 0..300 {
        let n = 2 + (case % 23);
        let w: Vec<u32> = (0..n)
            .map(|_| {
                lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                1 + ((lcg >> 33) % 1000) as u32
            })
            .collect();
        let total: f64 = w.iter().map(|&x| x as f64).sum();
        let exp: Vec<f64> = w.iter().map(|&x| x as f64 / total).collect();
        let w32: Vec<f32> = w.iter().map(|&x| x as f32 / 7.0).collect();
        let w64: Vec<f64> = w.iter().map(|&x| x as f64 / 7.0).collect();
        let trials = 200_000;
        let mut rng = Sfc32::new(case as u32);
        let mut s = Alias32::new(&mut rng, &w32);
        let f = freq(|| s.sample(), n, trials);
        let dev = f.iter().zip(&exp).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        if dev > 0.01 {
            flagged32 += 1;
        }
        if dev > worst32.0 {
            worst32 = (dev, w.clone());
        }
        let mut rng = Sfc64::new(case as u64);
        let mut s = Alias64::new(&mut rng, &w64);
        let f = freq(|| s.sample(), n, trials);
        let dev = f.iter().zip(&exp).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        if dev > 0.01 {
            flagged64 += 1;
        }
        if dev > worst64.0 {
            worst64 = (dev, w.clone());
        }
    }
    println!("Alias32 sweep: {flagged32}/300 weight vectors with max|dev|>0.01; worst {:.4} for {:?}", worst32.0, worst32.1);
    println!("Alias64 sweep: {flagged64}/300 weight vectors with max|dev|>0.01; worst {:.4} for {:?}", worst64.0, worst64.1);
}

#[cfg(target_arch = "x86_64")]
fn simdconv() {
    use std::arch::x86_64::*;
    unsafe fn arr<V, T: Copy, const N: usize>(v: V) -> [T; N] {
        unsafe { std::mem::transmute_copy(&v) }
    }
    struct Fixed(u32);
    impl RngV for Fixed {
        type Word = __m128i;
        fn nextuv(&mut self) -> __m128i {
            unsafe { _mm_set1_epi32(self.0 as i32) }
        }
    }
    let cases_i: [(u32, i32); 5] = [(0, 0), (0, i32::MIN), (10, i32::MAX - 2), (7, -3), (u32::MAX, 0)];
    for &(scale, min) in &cases_i {
        let r = catch_unwind(AssertUnwindSafe(|| unsafe {
            let mut g = Fixed(0xDEAD_BEEF);
            arr::<_, i32, 4>(g.randiv(_mm_set1_epi32(scale as i32), _mm_set1_epi32(min)))
        }));
        let r2 = unsafe { arr::<_, i32, 4>(WordV::to_randi(_mm_set1_epi32(0xDEAD_BEEFu32 as i32), _mm_set1_epi32(scale as i32), _mm_set1_epi32(min))) };
        println!("randiv(scale={scale:#x}, min={min}) -> {}  | to_randi -> {:?}",
            match r { Ok(v) => format!("{v:?}"), Err(_) => "PANIC".into() }, r2);
    }
    let cases_f: [(f32, f32); 6] = [(1.0, 0.0), (1.0, 16_777_216.0), (0.0, 0.0), (-1.0, 0.0), (f32::INFINITY, 0.0), (1e-30, 1.0)];
    for &(scale, min) in &cases_f {
        for &w in &[0u32, u32::MAX] {
            let r = catch_unwind(AssertUnwindSafe(|| unsafe {
                let mut g = Fixed(w);
                arr::<_, f32, 4>(g.randfv(_mm_set1_ps(scale), _mm_set1_ps(min)))
            }));
            println!("randfv(scale={scale:e}, min={min:e}) word={w:#x} -> {}",
                match r { Ok(v) => format!("{:e} (upper bound min+scale={:e}, value<upper: {})", v[0], min + scale, v[0] < min + scale), Err(_) => "PANIC".into() });
        }
    }
    // RngW (wide) randi/randf over full and degenerate ranges
    use urng::wide::RngW;
    let mut w = urng::wide::Xoshiro128Ppx8::new(3);
    let a = w.randi(i32::MIN, i32::MAX);
    let b = w.randi(4, 4);
    let c = w.randf(1.0, 1.0f32.next_up());
    println!("RngW randi(full)={a:?}\n RngW randi(4,4)={b:?}\n RngW randf(1,next_up(1))={c:?}");
}

fn randcore() {
    use rand_core::{Rng as _, SeedableRng};
    let mut a = Xoshiro128Pp::new(11);
    let mut b = Xoshiro128Pp::new(11);
    let w0 = urng::Rng::nextu(&mut a) as u64;
    let w1 = urng::Rng::nextu(&mut a) as u64;
    let v = b.next_u64();
    println!("32-bit TryRng next_u64 = {v:#018x}; words w0={w0:#010x} w1={w1:#010x}; first-word-high={} (rand_core utils convention is first-word-LOW: {})",
        v == (w0 << 32) | w1, v == (w1 << 32) | w0);
    let mut c = Xoshiro128Pp::new(11);
    let mut bytes = [0u8; 8];
    c.fill_bytes(&mut bytes);
    println!("fill_bytes(8) as LE u64 = {:#018x} (equals next_u64: {})", u64::from_le_bytes(bytes), u64::from_le_bytes(bytes) == v);
    let s = <Xoshiro128Pp as SeedableRng>::from_seed([1, 0, 0, 0]);
    let t = Xoshiro128Pp::new(1);
    println!("SeedableRng::from_seed([1,0,0,0]) == new(1): {}", first2(s) == first2(t));
    #[cfg(feature = "v_sol")]
    {
        let mut x = Xoshiro256Pp::new(5);
        let mut y = Xoshiro256Pp::new(5);
        let n = x.next_u64();
        println!("64-bit TryRng implemented (Sol): next_u64 == nextu: {}", n == urng::Rng::nextu(&mut y));
    }
}
fn first2<R: urng::Rng>(mut r: R) -> Vec<R::Word> {
    (0..2).map(|_| r.nextu()).collect()
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("randi_enum") => randi_enum(),
        Some("randi64_edge") => randi64_edge(),
        Some("randf_edge") => randf_edge(),
        Some("index") => index(),
        Some("shuffle") => shuffle(),
        Some("sampler") => sampler(),
        #[cfg(target_arch = "x86_64")]
        Some("simdconv") => simdconv(),
        Some("randcore") => randcore(),
        other => panic!("unknown probe {other:?}"),
    }
}

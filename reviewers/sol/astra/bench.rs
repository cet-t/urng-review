use std::{hint::black_box, time::Instant};
use urng::Rng;

fn main() {
    println!("case,elements,repeat,ns_per_word,checksum");
    macro_rules! scalar {
        ($ty:ident) => {
            for repeat in 0..5 {
                let mut rng = urng::$ty::new(black_box(1));
                let mut sum = 0u64;
                let n = 1usize << 24;
                let start = Instant::now();
                for _ in 0..n { sum ^= rng.nextu() as u64; }
                let elapsed = start.elapsed().as_nanos();
                black_box(sum);
                println!("scalar_{},{n},{repeat},{:.6},{sum}", stringify!($ty), elapsed as f64/n as f64);
            }
        }
    }
    scalar!(SplitMix64);
    scalar!(Xoshiro256Pp);
    scalar!(Xorshift64);
    scalar!(Threefry32x4);

    macro_rules! bulk {
        ($name:literal, $new:ident, $fill:ident, $free:ident, $word:ty) => {
            for n in [256usize, 65_536, 4_194_304] {
                let iterations = ((1usize << 24)/n).min(2048).max(4);
                let mut buffer = vec![0 as $word; n];
                for repeat in 0..5 {
                    let handle = urng::$new(1);
                    unsafe { urng::$fill(handle, buffer.as_mut_ptr(), n); }
                    let start = Instant::now();
                    for _ in 0..iterations {
                        unsafe { urng::$fill(handle, buffer.as_mut_ptr(), n); }
                        black_box(&buffer);
                    }
                    let elapsed = start.elapsed().as_nanos();
                    unsafe { urng::$free(handle); }
                    println!("{},{n},{repeat},{:.6},{}", $name, elapsed as f64/(n*iterations) as f64, black_box(buffer[n-1]));
                }
            }
        }
    }
    bulk!("cabi_splitmix64", splitmix64_new, splitmix64_next_u64s, splitmix64_free, u64);
    bulk!("cabi_xorshift64", xorshift64_new, xorshift64_next_u64s, xorshift64_free, u64);
    bulk!("cabi_threefry32x4", threefry32x4_new, threefry32x4_next_u32s, threefry32x4_free, u32);
}

use std::{hint::black_box, time::Instant};
use urng::{Rng, wide::RngW};

fn measure(name:&str,n:usize,mut f:impl FnMut()) {
    f();
    for trial in 0..7 {
        let start=Instant::now();f();
        println!("{name},{n},{trial},{:.6}",start.elapsed().as_secs_f64()*1e9/n as f64);
    }
}
fn main() {
    let n=1<<22;
    println!("case,words,trial,ns_per_word");
    let mut r=urng::SplitMix64::new(19);
    measure("scalar_splitmix64",n,||{let mut sum=0u64;for _ in 0..n{sum=sum.wrapping_add(r.nextu());}black_box(sum);});
    let mut r=urng::Threefry32x4::new(19);
    measure("scalar_threefry4",n,||{let mut sum=0u32;for _ in 0..n{sum=sum.wrapping_add(r.nextu());}black_box(sum);});
    let mut r=urng::wide::SplitMix32x8::new(19);
    measure("wide_splitmix32x8",n,||{let mut sum=[0u32;8];for _ in 0..n/8{let v=r.nextu();for i in 0..8{sum[i]=sum[i].wrapping_add(v[i]);}}black_box(sum);});
    let mut r=urng::wide::Sfc32x8::new(19);
    measure("wide_sfc32x8",n,||{let mut sum=[0u32;8];for _ in 0..n/8{let v=r.nextu();for i in 0..8{sum[i]=sum[i].wrapping_add(v[i]);}}black_box(sum);});
    let mut r=urng::wide::Sfc32x8::new(19);
    measure("wide_sfc32x8_randi10",n,||{let mut sum=[0i32;8];for _ in 0..n/8{let v=r.randi(0,9);for i in 0..8{sum[i]=sum[i].wrapping_add(v[i]);}}black_box(sum);});
    assert!(std::arch::is_x86_feature_detected!("avx512f"));
    let mut r=unsafe {urng::Sfc64x8::new(19)};
    measure("simd_sfc64x8",n,||{let mut sum=[0u64;8];for _ in 0..n/8{let v=unsafe{r.nextu()};for i in 0..8{sum[i]=sum[i].wrapping_add(v[i]);}}black_box(sum);});
    let mut r=urng::Sfc32::new(19);
    measure("scalar_sfc32_randi10",n,||{let mut sum=0i32;for _ in 0..n{sum=sum.wrapping_add(r.randi(black_box(0),black_box(9)));}black_box(sum);});
    unsafe {
        let count=1<<23;let mut v=vec![0u32;count];
        let h=urng::cabi::splitmix32_new(19);
        measure("cabi_splitmix32_32MiB",count,||{urng::cabi::splitmix32_next_u32s(h,v.as_mut_ptr(),count);black_box(&v);});
        urng::cabi::splitmix32_free(h);
        let h=urng::cabi::threefry32x4_new(19);
        measure("cabi_threefry4_32MiB",count,||{urng::cabi::threefry32x4_next_u32s(h,v.as_mut_ptr(),count);black_box(&v);});
        urng::cabi::threefry32x4_free(h);
        let h=urng::cabi::sfc32_new(19);
        measure("cabi_sfc32_32MiB",count,||{urng::cabi::sfc32_next_u32s(h,v.as_mut_ptr(),count);black_box(&v);});
        urng::cabi::sfc32_free(h);
    }
}

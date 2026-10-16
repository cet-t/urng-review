//! Independent review tests. Only installed in a disposable source copy.
use crate::*;
use crate::wide::RngW;
use wrapn::wrap;
include!("adapter.rs");

#[test]
fn environment() {
    #[cfg(target_arch="x86_64")]
    unsafe {
        use std::arch::x86_64::*;
        let mut bytes = Vec::new();
        for leaf in 0x80000002..=0x80000004 {
            let r = __cpuid(leaf);
            for x in [r.eax,r.ebx,r.ecx,r.edx] { bytes.extend(x.to_le_bytes()); }
        }
        println!("CPU={} avx2={} avx512f={} avx512dq={} avx512vl={} threads={:?}",
            String::from_utf8_lossy(&bytes), is_x86_feature_detected!("avx2"),
            is_x86_feature_detected!("avx512f"), is_x86_feature_detected!("avx512dq"),
            is_x86_feature_detected!("avx512vl"), std::thread::available_parallelism());
    }
}

#[test]
fn extreme_float32() {
    for w in [0, 1, 0x40000000, 0x80000000, u32::MAX] {
        let x = w.to_randf(-f32::MAX, f32::MAX);
        println!("float32 word={w:08x} -> {x}");
        assert!(x.is_finite() && x >= -f32::MAX && x < f32::MAX);
        if w == 0 { assert_eq!(x, -f32::MAX); }
    }
}

#[test]
fn extreme_float64() {
    for w in [0, 1, 0x2000000000000000, 0x4000000000000000, u64::MAX] {
        let x = w.to_randf(-f64::MAX, f64::MAX);
        println!("float64 word={w:016x} -> {x}");
        assert!(x.is_finite() && x >= -f64::MAX && x < f64::MAX);
        if w == 0 { assert_eq!(x, -f64::MAX); }
    }
}

#[test]
fn regular_float_and_integer_edges() {
    for w in [0u32,1,0x7fffffff,0x80000000,u32::MAX] {
        assert!((0.0..1.0).contains(&w.to_f01()));
        assert_eq!(w.to_randi(i32::MIN, i32::MAX), (w as i32).wrapping_add(i32::MIN));
        let f = w.to_randf(100.0, 101.0);
        assert!((100.0..101.0).contains(&f));
    }
    for w in [0u64,1,0x7fffffffffffffff,0x8000000000000000,u64::MAX] {
        assert!((0.0..1.0).contains(&w.to_f01()));
        assert_eq!(w.to_randi(i64::MIN, i64::MAX), (w as i64).wrapping_add(i64::MIN));
        assert!((100.0..101.0).contains(&w.to_randf(100.0,101.0)));
    }
}

#[test]
fn alias32_subnormal_zero_weight() {
    let mut rng = Mt19937::new(17);
    let mut sampler = Alias32::new(&mut rng, &[0.0, f32::from_bits(1)]);
    let mut count = [0;2];
    for _ in 0..10000 { count[sampler.sample()] += 1; }
    println!("Alias32 [0, min-subnormal]: {count:?}");
    assert_eq!(count,[0,10000]);
}
#[test]
fn alias64_subnormal_zero_weight() {
    let mut rng = Mt1993764::new(17);
    let mut sampler = Alias64::new(&mut rng, &[0.0, f64::from_bits(1)]);
    let mut count = [0;2];
    for _ in 0..10000 { count[sampler.sample()] += 1; }
    println!("Alias64 [0, min-subnormal]: {count:?}");
    assert_eq!(count,[0,10000]);
}

#[test]
fn samplers_invalid_and_regular() {
    for weights in [vec![], vec![0.0,0.0], vec![1.0,-1.0], vec![f32::NAN], vec![f32::INFINITY]] {
        assert!(std::panic::catch_unwind(|| { let mut r=Mt19937::new(3); let _=Alias32::new(&mut r,&weights); }).is_err());
        assert!(std::panic::catch_unwind(|| { let mut r=Mt19937::new(3); let _=Bst32::new(&mut r,&weights); }).is_err());
    }
    let mut rng = Mt19937::new(3);
    let mut sampler=Bst32::new(&mut rng,&[0.0,1.0,0.0,3.0,0.0]);
    let mut count=[0usize;5];
    for _ in 0..100000 { count[sampler.sample()]+=1; }
    assert_eq!(count[0]+count[2]+count[4],0);
    assert!((count[1] as f64 / 100000.0-0.25).abs()<0.01);
}

#[test]
fn wide_xorshift_zero_lane() {
    let mut rng = wide::Xorshift32x4::new(0x61c88647);
    let first: Vec<_> = (0..32).map(|_|rng.nextu()[0]).collect();
    println!("wide xorshift lane0 seed61c88647={first:?}");
    assert!(first.iter().any(|&x|x!=0));
}

#[test]
fn scalar_xorshift_zero_seeds() {
    for seed in [0x61c88647,0xd7bdef1b,0xf7aa06c3] {
        let mut rng=Xorshift32::new(seed);
        assert!((0..32).any(|_|rng.nextu()!=0));
    }
    let mut rng=Xorshift64::new(0x61c8864680b583eb);
    assert!((0..32).any(|_|rng.nextu()!=0));
}

// Independently structured reference rounds; KATs below check the reference too.
fn tf4(mut x:[u32;4], key:[u32;4])->[u32;4] {
    let k=[key[0],key[1],key[2],key[3],0x1bd11bda^key[0]^key[1]^key[2]^key[3]];
    let rotations=[[10,26],[11,21],[13,27],[23,5],[6,20],[17,11],[25,10],[18,20]];
    for i in 0..4 { x[i]=x[i].wrapping_add(k[i]); }
    for round in 0..20 {
        for pair in 0..2 {
            let a=2*pair; let b=a+1;
            x[a]=x[a].wrapping_add(x[b]);
            x[b]=x[b].rotate_left(rotations[round%8][pair])^x[a];
        }
        x.swap(1,3);
        if round%4==3 {
            let s=round/4+1;
            for i in 0..4 { x[i]=x[i].wrapping_add(k[(s+i)%5]); }
            x[3]=x[3].wrapping_add(s as u32);
        }
    }
    x
}

#[test]
fn random123_known_answers() {
    // https://github.com/DEShawResearch/random123/blob/main/tests/kat_vectors
    let cases=[([0;4],[0;4],[0x9c6ca96a,0xe17eae66,0xfc10ecd4,0x5256a7d8]),
        ([u32::MAX;4],[u32::MAX;4],[0x2a881696,0x57012287,0xf6c7446e,0xa16a6732]),
        ([0x243f6a88,0x85a308d3,0x13198a2e,0x03707344],[0xa4093822,0x299f31d0,0x082efa98,0xec4e6c89],[0x59cd1dbb,0xb8879579,0x86b5d00c,0xac8b6d84])];
    for (c,k,want) in cases {
        assert_eq!(tf4(c,k),want);
        let mut r=Threefry32x4::new(1);
        r.c=c.map(|x|wrap!(x));
        r.k=[k[0],k[1],k[2],k[3],k.iter().fold(0x1bd11bda,|a,b|a^b)].map(|x|wrap!(x));
        assert_eq!(r.next_raw().map(|x|*x),want);
    }
    let mut p=Philox32::new(1); p.c=wrap![0;4]; p.k=wrap![0;2];
    assert_eq!(p.next_raw().map(|x|*x),[0x6627e8d5,0xe169c58d,0xbc57ac4c,0x9b00dbd8]);
    let mut p=Philox64::new(1); p.c=wrap![0;2]; p.k=wrap![0;2];
    assert_eq!(p.next_raw().map(|x|*x),[0xca00a0459843d731,0x66c24222c9a845b5]);
    let mut p=Threefry32x2::new(1);p.c=wrap![0;2];p.k=wrap![0,0,0x1bd11bda];
    assert_eq!(p.next_raw().map(|x|*x),[0x6b200159,0x99ba4efe]);
}

#[test]
fn wide_threefry4_reference() {
    // Obtain only the documented seed expansion from scalar SplitMix32;
    // the reference block function is independent and validated above.
    let mut sm=SplitMix32::new(19);
    let keys:[[u32;4];4]=std::array::from_fn(|_|std::array::from_fn(|_|sm.nextu()));
    let mut r=wide::Threefry32x4x4::new(19);
    for counter in 0..100 {
        let expected:[[u32;4];4]=std::array::from_fn(|lane|tf4([counter,0,0,0],std::array::from_fn(|i|keys[i][lane])));
        for word in 0..4 {
            let got=r.nextu();
            for lane in 0..4 { assert_eq!(got[lane],expected[lane][word],"counter={counter} word={word} lane={lane}"); }
        }
    }
}

#[test]
fn counter_carry_scalar() {
    let mut p=Philox32::new(1);p.c=wrap![u32::MAX,u32::MAX,7,9];
    let _=p.next_raw();assert_eq!(p.c.map(|x|*x),[0,0,8,9]);
    let mut t=Threefry32x4::new(1);t.c=wrap![u32::MAX,u32::MAX,u32::MAX,9];
    let _=t.next_raw();assert_eq!(t.c.map(|x|*x),[0,0,0,10]);
    let mut t=Threefry32x2::new(1);t.c=wrap![u32::MAX,9];
    let _=t.next_raw();assert_eq!(t.c.map(|x|*x),[0,10]);
    let mut p=Philox64::new(1);p.c=wrap![u64::MAX,9];
    let _=p.next_raw();assert_eq!(p.c.map(|x|*x),[0,10]);
}

#[test]
fn chacha_rfc8439() {
    let mut r=chacha(std::array::from_fn(|i|i as u8),[0,0,0,9,0,0,0,0x4a,0,0,0,0],1);
    let expected=[0xe4e7f110,0x15593bd1,0x1fdd0f50,0xc47120a3,0xc7f4d1c7,0x0368c033,0x9aaa2204,0x4e6cd4c3,0x466482d2,0x09aa9f07,0x05d7c214,0xa2028bd9,0xd19c12b5,0xb94e16de,0xe883d0cb,0x4e3c50a2];
    let got:[u32;16]=std::array::from_fn(|_|r.nextu());
    assert_eq!(got,expected);
}

fn salsa_ref(key:[u8;32],nonce:[u8;8],counter:u64)->[u32;16] {
    let mut input=[0;16];
    for (i,c) in [0x61707865,0x3320646e,0x79622d32,0x6b206574].into_iter().enumerate() {input[5*i]=c;}
    for i in 0..8 {input[if i<4 {i+1}else{i+7}]=u32::from_le_bytes(key[i*4..i*4+4].try_into().unwrap());}
    input[6]=u32::from_le_bytes(nonce[..4].try_into().unwrap());input[7]=u32::from_le_bytes(nonce[4..].try_into().unwrap());
    input[8]=counter as u32;input[9]=(counter>>32)as u32;
    let mut x=input;
    for _ in 0..10 {
        for [a,b,c,d] in [[0,4,8,12],[5,9,13,1],[10,14,2,6],[15,3,7,11],[0,1,2,3],[5,6,7,4],[10,11,8,9],[15,12,13,14]] {
            x[b]^=x[a].wrapping_add(x[d]).rotate_left(7);
            x[c]^=x[b].wrapping_add(x[a]).rotate_left(9);
            x[d]^=x[c].wrapping_add(x[b]).rotate_left(13);
            x[a]^=x[d].wrapping_add(x[c]).rotate_left(18);
        }
    }
    std::array::from_fn(|i|x[i].wrapping_add(input[i]))
}

#[test]
fn salsa_reference_multiblock_and_carry() {
    let key=std::array::from_fn(|i|i as u8);let nonce=[1,2,3,4,5,6,7,8];
    let mut r=salsa(key,nonce,u32::MAX as u64-1);
    for counter in u32::MAX as u64-1..u32::MAX as u64+3 {
        let got:[u32;16]=std::array::from_fn(|_|r.nextu());
        assert_eq!(got,salsa_ref(key,nonce,counter));
    }
}

#[test]
fn crypto_debug_redacts_secret() {
    let c=chacha([0x42;32],[0;12],0);
    let s=salsa([0x42;32],[0;8],0);
    let a=format!("{c:?}");let b=format!("{s:?}");
    println!("ChaCha Debug: {a}\nSalsa Debug: {b}");
    // 0x42424242 is 1111638594, the eight key words.
    assert!(!a.contains("1111638594") && !b.contains("1111638594"));
}

#[test]
fn cabi_exact_stream_and_guards() {
    use crate::cabi::*;
    unsafe {
        let h=splitmix32_new(71);let mut r=SplitMix32::new(71);
        for count in [0,1,3,16,8191,8192,8193,131073] {
            let mut v=vec![0xdeadbeefu32;count+2];
            splitmix32_next_u32s(h,v.as_mut_ptr().add(1),count);
            assert_eq!(v[0],0xdeadbeef);assert_eq!(v[count+1],0xdeadbeef);
            for &x in &v[1..count+1] {assert_eq!(x,r.nextu());}
        }
        splitmix32_free(h);
        let h=threefry32x4_new(71);let mut r=Threefry32x4::new(71);
        for count in [1,3,5,8193,131073] {
            let mut v=vec![0xdeadbeefu32;count+2];
            threefry32x4_next_u32s(h,v.as_mut_ptr().add(1),count);
            assert_eq!(v[0],0xdeadbeef);assert_eq!(v[count+1],0xdeadbeef);
            for &x in &v[1..count+1] {assert_eq!(x,r.nextu());}
        }
        threefry32x4_free(h);
    }
}

#[test]
fn integer_distribution_scalar() {
    let mut r=Mt19937::new(5);let mut counts=[0usize;3];
    for _ in 0..300000 {let x=r.randi(i32::MIN,0x3fffffff);counts[((x as i64-i32::MIN as i64)%3)as usize]+=1;}
    println!("scalar integer residues: {counts:?}");
    for c in counts {assert!((c as f64/300000.0-1.0/3.0).abs()<0.01);}
}

#[test]
fn wide_threefry2_carry() {
    let mut r=wide::Threefry32x2x4::new(19);
    let initial=r.nextu();
    r.astra_set_counter(u32::MAX);
    let _=r.nextu();let _=r.nextu();
    let after=r.nextu();
    println!("Threefry2 initial={initial:?}, after low-counter wrap={after:?}");
    assert_ne!(after,initial,"must carry into high counter instead of restarting the stream");
}

#[test]
fn wide_threefry4_carry() {
    let mut r=wide::Threefry32x4x4::new(19);
    let initial=r.nextu();
    r.astra_set_counter(u32::MAX);
    for _ in 0..4 {let _=r.nextu();}
    let after=r.nextu();
    println!("Threefry4 initial={initial:?}, after low-counter wrap={after:?}");
    assert_ne!(after,initial,"must carry into high counter instead of restarting the stream");
}

#[test]
fn wide_squares_zero_key() {
    let mut r=wide::Squares32x4::new(0x61c8864680b583eb);
    let lane:Vec<_>=(0..32).map(|_|r.nextu()[0]).collect();
    println!("Squares32x4 lane0={lane:?}");
    assert!(lane.iter().any(|&x|x!=0),"zero-key lane is absorbing");
}

#[test]
fn neighboring_seeds_distinct() {
    let mut a=SplitMix32::new(0);let mut b=SplitMix32::new(1);
    let x:Vec<_>=(0..32).map(|_|a.nextu()).collect();let y:Vec<_>=(0..32).map(|_|b.nextu()).collect();
    let mut a=SplitMix64::new(0);let mut b=SplitMix64::new(1);
    let p:Vec<_>=(0..32).map(|_|a.nextu()).collect();let q:Vec<_>=(0..32).map(|_|b.nextu()).collect();
    println!("neighbor seeds: SplitMix32 equal={}, SplitMix64 equal={}",x==y,p==q);
    assert!(x!=y && p!=q);
}

fn xoroshiro_step(s: u128, pp: bool)->u128 {
    let a=s as u64; let b=(s>>64)as u64 ^ a;
    let (r,l,t)=if pp{(49,21,28)}else{(24,16,37)};
    (a.rotate_left(r)^b^(b<<l)) as u128 | ((b.rotate_left(t) as u128)<<64)
}
fn matrix_apply(m:&[u128;128],mut x:u128)->u128 {
    let mut out=0;
    while x!=0 {let i=x.trailing_zeros() as usize;out^=m[i];x&=x-1;}
    out
}
fn power_of_two_jump(s:u128,power:usize,pp:bool)->u128 {
    let mut m=std::array::from_fn(|i|xoroshiro_step(1u128<<i,pp));
    for _ in 0..power {m=std::array::from_fn(|i|matrix_apply(&m,m[i]));}
    matrix_apply(&m,s)
}
#[test]
fn xoroshiro_jumps_independent_matrix() {
    for seed in [1,19,0x123456789abcdef] {
        let mut sm=SplitMix64::new(seed);let s=sm.nextu()as u128|((sm.nextu()as u128)<<64);
        for power in [64,96] {
            let pp=power_of_two_jump(s,power,true);let ss=power_of_two_jump(s,power,false);
            let mut a=Xoroshiro128Pp::new(seed);let mut b=Xoroshiro128Ss::new(seed);
            if power==64 {a.jump();b.jump();}else{a.long_jump();b.long_jump();}
            let pp0=pp as u64;let pp1=(pp>>64)as u64;
            assert_eq!(a.nextu(),pp0.wrapping_add(pp1).rotate_left(17).wrapping_add(pp0));
            assert_eq!(b.nextu(),(ss as u64).wrapping_mul(5).rotate_left(7).wrapping_mul(9));
        }
    }
}

#[test]
fn simd_sfc64_lane_reference() {
    if !std::arch::is_x86_feature_detected!("avx512f") {return;}
    let mut seed=SplitMix64::new(19);
    let mut states:[[u64;4];8]=std::array::from_fn(|_|[seed.nextu(),seed.nextu(),seed.nextu(),1]);
    let mut rng=unsafe{Sfc64x8::new(19)};
    for _ in 0..1000 {
        let got=unsafe{rng.nextu()};
        for lane in 0..8 {
            let [a,b,c,n]=states[lane];let out=a.wrapping_add(b).wrapping_add(n);
            states[lane]=[b^(b>>11),c.wrapping_mul(9),c.rotate_left(24).wrapping_add(out),n.wrapping_add(1)];
            assert_eq!(got[lane],out);
        }
    }
}

fn philox_ref(mut c:[u32;4],mut k:[u32;2])->[u32;4] {
    for _ in 0..10 {
        let a=c[0]as u64*0xd2511f53;let b=c[2]as u64*0xcd9e8d57;
        c=[(b>>32)as u32^c[1]^k[0],b as u32,(a>>32)as u32^c[3]^k[1],a as u32];
        k[0]=k[0].wrapping_add(0x9e3779b9);k[1]=k[1].wrapping_add(0xbb67ae85);
    }
    c
}
#[test]
fn simd_philox_lane_reference() {
    assert_eq!(philox_ref([0;4],[0;2]),[0x6627e8d5,0xe169c58d,0xbc57ac4c,0x9b00dbd8]);
    if !std::arch::is_x86_feature_detected!("avx512f") {return;}
    let mut seed=SplitMix32::new(19);
    let counters:[u32;16]=std::array::from_fn(|_|seed.nextu());
    let keys:[u32;8]=std::array::from_fn(|_|seed.nextu());
    let mut rng=unsafe{Philox32x4x4::new(19)};
    for t in 0..1000u128 {
        let got=unsafe{rng.nextu()};
        for lane in 0..4 {
            let c=(0..4).fold(0u128,|a,j|a|((counters[lane*4+j]as u128)<<(32*j))).wrapping_add(t);
            let want=philox_ref(std::array::from_fn(|j|(c>>(32*j))as u32),[keys[lane*2],keys[lane*2+1]]);
            assert_eq!(&got[lane*4..lane*4+4],&want);
        }
    }
}

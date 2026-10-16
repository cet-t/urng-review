// A compile canary: public 64-bit generators must be usable via rand_core.
fn require_rng<T: rand_core::TryRng>() {}
#[test]
fn native_64_bit_generators_implement_rand_core() {
    require_rng::<urng::SplitMix64>();
    require_rng::<urng::Xoshiro256Pp>();
    require_rng::<urng::Sfc64>();
    require_rng::<urng::Philox4x64>();
}

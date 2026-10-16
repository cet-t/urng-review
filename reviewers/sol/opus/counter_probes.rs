// Appended only to the disposable copy's wide/threefry.rs by run_checks.py.
// These tests inject a valid state immediately before low-word overflow.
#[cfg(test)]
mod sol_counter_probes {
    use super::*;

    #[test]
    fn threefry2_carries_into_high_counter_word() {
        let mut r = Threefry32x2x4::new(1);
        r.c0 = u32x4::splat(u32::MAX);
        let _ = r.nextu();
        let _ = r.nextu();
        let wrapped = r.nextu();
        let first = Threefry32x2x4::new(1).nextu();
        eprintln!("2x32 output after overflow={wrapped:?}, initial={first:?}");
        assert_eq!(r.c1.to_array(), [1; 4], "high counter must advance; output repeats at 2^32 blocks");
    }

    #[test]
    fn threefry4_carries_into_high_counter_word() {
        let mut r = Threefry32x4x4::new(1);
        r.c0 = u32x4::splat(u32::MAX);
        for _ in 0..4 { let _ = r.nextu(); }
        let wrapped = r.nextu();
        let first = Threefry32x4x4::new(1).nextu();
        eprintln!("4x32 output after overflow={wrapped:?}, initial={first:?}");
        assert_eq!(r.c1.to_array(), [1; 4], "high counter must advance; output repeats at 2^32 blocks");
    }
}

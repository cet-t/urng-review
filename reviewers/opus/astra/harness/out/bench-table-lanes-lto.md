| case | lanes-lto-base ns/word | lanes-lto-sol ns/word | lanes-lto-astra ns/word | lanes-lto-sol/lanes-lto-base | lanes-lto-astra/lanes-lto-base | max run spread |
|---|---|---|---|---|---|---|
| `simd::Sfc32x16::nextu` | 0.2373 | 0.2402 | 0.2360 | 1.01 | 0.99 | 1.2% |
| `simd::Sfc32x8::nextu` | 0.7495 | 0.4198 | 0.4049 | 0.56 | 0.54 | 5.9% |
| `simd::Xoshiro128Ppx16::nextuv` | 3.9889 | 4.0931 | 3.9976 | 1.03 | 1.00 | 0.6% |
| `simd::Jsf32x16::nextuv` | 2.8480 | 2.8942 | 2.8382 | 1.02 | 1.00 | 3.4% |
| `simd::SplitMix32x16::nextuv` | 0.8992 | 1.9169 | 0.6675 | 2.13 | 0.74 | 1.0% |
| `simd::Pcg32x8::nextu` | 0.5478 | 0.5473 | 0.5474 | 1.00 | 1.00 | 0.5% |
| `simd::Philox32x4x4::nextu` | 0.4823 | 1.2949 | 1.2536 | 2.68 | 2.60 | 1.3% |
| `simd::Squares32x8::nextu` | 1.1589 | 1.1628 | 1.1616 | 1.00 | 1.00 | 0.8% |
| `simd::Biski64x8::nextu` | 3.6371 | 3.9547 | 0.3944 | 1.09 | 0.11 | 0.5% |
| `simd::Sfc64x8::nextu` | 6.9084 | 6.7978 | 0.4748 | 0.98 | 0.07 | 5.0% |
| `simd::Cet64x8::nextu` | 0.3014 | 0.3020 | 0.3117 | 1.00 | 1.03 | 6.6% |
| `wide::Xoshiro128Ppx16::nextu` | 0.2820 | 0.2842 | 0.2843 | 1.01 | 1.01 | 0.6% |
| `wide::Sfc32x16::nextu` | 0.2319 | 0.2284 | 0.2329 | 0.98 | 1.00 | 2.1% |
| `wide::SplitMix32x16::nextu` | 0.3716 | 0.6314 | 0.6664 | 1.70 | 1.79 | 6.4% |
| `wide::Jsf32x16::nextu` | 0.2393 | 0.2375 | 0.2434 | 0.99 | 1.02 | 5.5% |
| `wide::Threefry32x4x16::nextu` | 1.0099 | 1.0195 | 1.0393 | 1.01 | 1.03 | 5.3% |
| `wide::Pcg32x16::nextu` | 0.4501 | 0.4496 | 0.4632 | 1.00 | 1.03 | 4.8% |
| `wide::Xoshiro128Ppx16::randi(0,999)` | 0.3700 | 0.8114 | 0.8881 | 2.19 | 2.40 | 5.2% |

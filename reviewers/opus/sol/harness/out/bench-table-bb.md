| case | bb-base ns/word | bb-sol ns/word | bb-astra ns/word | bb-sol/bb-base | bb-astra/bb-base | max run spread |
|---|---|---|---|---|---|---|
| `rust::Sfc32::randi(bb 0,999)` | 1.1554 | 2.1787 | 6.4268 | 1.89 | 5.56 | 89.9% |
| `rust::Xoshiro256Pp::randi(bb 0,999)` | 1.3211 | 2.4510 | 4.1947 | 1.86 | 3.18 | 52.5% |
| `rust::Sfc32::randf(bb 0,1)` | 0.9453 | 2.1867 | 3.0735 | 2.31 | 3.25 | 65.5% |
| `rust::Xoshiro256Pp::randf(bb -1,1)` | 1.0247 | 2.0368 | 3.0732 | 1.99 | 3.00 | 49.6% |

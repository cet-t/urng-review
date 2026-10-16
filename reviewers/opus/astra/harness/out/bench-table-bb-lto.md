| case | bb-lto-base ns/word | bb-lto-sol ns/word | bb-lto-astra ns/word | bb-lto-sol/bb-lto-base | bb-lto-astra/bb-lto-base | max run spread |
|---|---|---|---|---|---|---|
| `rust::Sfc32::randi(bb 0,999)` | 1.1598 | 1.9487 | 4.6050 | 1.68 | 3.97 | 19.1% |
| `rust::Xoshiro256Pp::randi(bb 0,999)` | 1.3418 | 2.3316 | 4.0225 | 1.74 | 3.00 | 27.2% |
| `rust::Sfc32::randf(bb 0,1)` | 1.0011 | 2.5680 | 2.6234 | 2.57 | 2.62 | 49.2% |
| `rust::Xoshiro256Pp::randf(bb -1,1)` | 0.9752 | 2.4603 | 3.5949 | 2.52 | 3.69 | 50.0% |

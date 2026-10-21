# Fuzz tests

Fuzz targets for the `.tmod` reader (ADR 0009 and ADR 0019). They need a nightly toolchain and cargo-fuzz.

```
cargo install cargo-fuzz
cargo +nightly fuzz run tmod_read -- -max_total_time=120
```

`corpus/tmod_read/` holds the seed packages. CI runs the target for 120 seconds on Linux. The stable test `crates/t-rustable-tmod/tests/mutation.rs` runs the same reader with random changes on all platforms.

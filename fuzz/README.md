# Fuzzing

This directory contains a [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz)
setup for `matchmaker-nucleo`.

## What it fuzzes

The single fuzz target (`fuzz_target_1`) exercises the core matcher
(`Matcher::fuzzy_indices` / `fuzzy_indices_greedy`) with arbitrary
haystack/needle pairs and random `ignore_case` / `normalize` configs.
It asserts the core matcher invariants:

- Greedy and optimal matching agree on whether a match exists.
- If both match, the optimal score is always >= the greedy score.
- If both select the same character indices, the scores are identical.
- Selected indices, after normalization/case-folding, reconstruct the needle.

## Prerequisites

- Rust stable
- `cargo install cargo-fuzz`
- A C/C++ compiler and LLVM toolchain (libFuzzer backend)

## How to run

The repo defines a convenience alias `cargo @fuzz` (requires cargo-alias-exec):

```sh
# Build the fuzz target (sanity check)
cargo @fuzz build

# Run the fuzzer indefinitely (corpus saved under fuzz/corpus)
cargo @fuzz run

# Run with a time budget (seconds), useful for CI smoke tests
cargo @fuzz run -- -max_total_time=60

# Re-run crashes found previously (stored under fuzz/artifacts)
cargo @fuzz run fuzz/artifacts/fuzz_target_1/crash-*
```

Or invoke cargo-fuzz directly from the repo root:

```sh
cargo +nightly fuzz build
cargo +nightly fuzz run fuzz_target_1
cargo +nightly fuzz run fuzz_target_1 -- -max_total_time=60
cargo +nightly fuzz help
```

Note: `cargo-fuzz` requires the nightly toolchain. Crashes are written to
`fuzz/artifacts/`, and interesting inputs that extend coverage are
persisted to `fuzz/corpus/` (both are git-ignored).


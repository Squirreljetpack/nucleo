#!/usr/bin/env bash
# Regenerates src/matcher_inner/chars/case_fold.rs from the Unicode
# Character Database (UCD 15.0.0). Requires: curl, unzip, cargo.
set -e

dir=$(pwd)
mkdir /tmp/ucd-15.0.0
cd /tmp/ucd-15.0.0
curl -LO https://www.unicode.org/Public/zipped/15.0.0/UCD.zip
unzip UCD.zip

cd "${dir}"
cargo install ucd-generate
ucd-generate case-folding-simple /tmp/ucd-15.0.0 --chars > src/matcher_inner/chars/case_fold.rs
rm -rf /tmp/ucd-15.0.0

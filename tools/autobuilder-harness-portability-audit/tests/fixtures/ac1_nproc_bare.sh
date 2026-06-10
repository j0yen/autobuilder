#!/bin/bash
# Script with bare nproc (no fallback)
JOBS=$(nproc)
cargo build --jobs "$JOBS"

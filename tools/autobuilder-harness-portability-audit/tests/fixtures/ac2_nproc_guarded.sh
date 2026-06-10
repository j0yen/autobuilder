#!/bin/bash
# Script with guarded nproc
JOBS=$(nproc 2>/dev/null || sysctl -n hw.logicalcpu || echo 4)
cargo build --jobs "$JOBS"

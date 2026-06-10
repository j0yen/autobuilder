#!/bin/bash
# Clean script with no Linux-only idioms
JOBS=$(nproc 2>/dev/null || sysctl -n hw.logicalcpu || echo 4)
SCRIPT_DIR=$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$0")
echo "Jobs: $JOBS"
echo "Script: $SCRIPT_DIR"

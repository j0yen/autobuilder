#!/bin/bash
# Script triggering all 7 rules

# proc-fs
CPU_COUNT=$(cat /proc/cpuinfo | grep processor | wc -l)

# flock
flock 200

# gnu-date
TOMORROW=$(date -d "tomorrow" +%Y-%m-%d)
NEXT_WEEK=$(date --date "next week" +%Y-%m-%d)

# readlink-f
SCRIPT_DIR=$(readlink -f "$0")

# sed-i-empty (no backup suffix)
sed -i 's/foo/bar/g' file.txt

# stat-c
SIZE=$(stat -c %s file.txt)

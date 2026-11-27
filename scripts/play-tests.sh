#!/bin/sh
# Plays the button replays in tests/scripts on the desktop build and reports
# which failed. Replays whose names share the part before the first "-"
# (saves-1-journey.txt, saves-2-resume.txt, ...) run in order on one fresh
# save folder; each group gets its own.
set -eu
cd "$(dirname "$0")/.."
cargo build --release --quiet
bin=target/release/littleprince
root=$(mktemp -d)
failed=0
for s in tests/scripts/*.txt; do
	name=$(basename "$s" .txt)
	group=${name%%-*}
	mkdir -p "$root/$group"
	if GIZLET_SAVE_DIR="$root/$group" GIZLET_LANG=zh LITTLEPRINCE_SCRIPT="$s" LITTLEPRINCE_MUTE=1 \
		LITTLEPRINCE_WINDOW=640x480 "$bin" >"$root/$name.log" 2>&1; then
		echo "ok   $name"
	else
		echo "FAIL $name"
		grep "script: FAIL\|panicked" "$root/$name.log" || tail -5 "$root/$name.log"
		failed=$((failed + 1))
	fi
done
rm -rf "$root"
[ "$failed" -eq 0 ]

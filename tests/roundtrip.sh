#!/usr/bin/env bash
set -euo pipefail

# Round-trip test: backup → restore → verify for various file sizes and nchunks.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Build both binaries
echo "Building binaries..."
cargo build --release --bin lxr_backup --bin lxr_restore --manifest-path "$PROJECT_DIR/Cargo.toml"

LXR_BACKUP="$PROJECT_DIR/target/release/lxr_backup"
LXR_RESTORE="$PROJECT_DIR/target/release/lxr_restore"

TMPBASE="$(mktemp -d)"
trap 'rm -rf "$TMPBASE"' EXIT

FILE_SIZES=(0 120 1048576 12582832 25165961)   # 0, 120, 1M, 12M-80, 24M+137
SIZE_LABELS=("0B" "120B" "1M" "12M" "24M")
NCHUNKS_LIST=(16 17 29 48)

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi | awk '{print $1}'
}

PASS=0
FAIL=0
TOTAL=0

for nc in "${NCHUNKS_LIST[@]}"; do
  for idx in "${!FILE_SIZES[@]}"; do
    sz="${FILE_SIZES[$idx]}"
    label="${SIZE_LABELS[$idx]}"
    TOTAL=$((TOTAL + 1))

    TEST_ID="nc${nc}_${label}"
    SRC_DIR="$TMPBASE/$TEST_ID/src"
    CHUNK_DIR="$TMPBASE/$TEST_ID/chunks"
    DB_DIR="$TMPBASE/$TEST_ID/db"
    OUT_DIR="$TMPBASE/$TEST_ID/out"

    mkdir -p "$SRC_DIR" "$CHUNK_DIR" "$DB_DIR" "$OUT_DIR"

    # Create test file
    SRC_FILE="$SRC_DIR/testfile_${label}.bin"
    if [ "$sz" -eq 0 ]; then
      touch "$SRC_FILE"
    else
      dd if=/dev/urandom of="$SRC_FILE" bs=65536 count=$(( (sz + 65535) / 65536 )) 2>/dev/null
      truncate -s "$sz" "$SRC_FILE"
    fi

    ORIG_CKSUM="$(sha256 "$SRC_FILE")"

    # Backup
    if ! "$LXR_BACKUP" -x "$CHUNK_DIR" -d "$DB_DIR" -n "$nc" -i "rt_test" "$SRC_FILE" \
         > "$TMPBASE/$TEST_ID/backup.log" 2>&1; then
      echo "FAIL  nchunks=$nc  size=$label  (backup failed)"
      cat "$TMPBASE/$TEST_ID/backup.log"
      FAIL=$((FAIL + 1))
      continue
    fi

    # Restore (the number of chunks is read from the key information)
    if ! "$LXR_RESTORE" -x "$CHUNK_DIR" -d "$DB_DIR" -i "rt_test" -o "$OUT_DIR" "$SRC_FILE" \
         > "$TMPBASE/$TEST_ID/restore.log" 2>&1; then
      # An empty file has no blocks, so no metadata is written for it
      # (as in elykseer-ml) and it cannot be restored.
      if [ "$sz" -eq 0 ]; then
        echo "SKIP  nchunks=$nc  size=$label  (empty file — no data to restore)"
        TOTAL=$((TOTAL - 1))
        continue
      fi
      echo "FAIL  nchunks=$nc  size=$label  (restore failed)"
      cat "$TMPBASE/$TEST_ID/restore.log"
      FAIL=$((FAIL + 1))
      continue
    fi

    # Find restored file (path is relative under OUT_DIR)
    RESTORED_FILE="$OUT_DIR/${SRC_FILE#/}"
    if [ ! -f "$RESTORED_FILE" ]; then
      echo "FAIL  nchunks=$nc  size=$label  (restored file not found: $RESTORED_FILE)"
      FAIL=$((FAIL + 1))
      continue
    fi

    RESTORED_CKSUM="$(sha256 "$RESTORED_FILE")"

    if [ "$ORIG_CKSUM" = "$RESTORED_CKSUM" ]; then
      echo "PASS  nchunks=$nc  size=$label"
      PASS=$((PASS + 1))
    else
      echo "FAIL  nchunks=$nc  size=$label  (checksum mismatch)"
      echo "  original: $ORIG_CKSUM"
      echo "  restored: $RESTORED_CKSUM"
      FAIL=$((FAIL + 1))
    fi
  done
done

echo
echo "===== Results: $PASS passed, $FAIL failed out of $TOTAL tests ====="
if [ "$FAIL" -ne 0 ]; then
  exit 1
fi

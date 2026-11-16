#!/usr/bin/env bash
# Command line tests of lxr_backup and lxr_restore: exit codes on success and
# on failure, and that a failed restore leaves no partial file behind.
# Follows test/test_cli.sh of elykseer-ml.
#
#   tests/test_cli.sh [directory with the binaries]
#
# Without an argument the release binaries are built and used.

set -u

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
if [ $# -ge 1 ]; then
  BIN="$(cd "$1" && pwd)"
else
  cargo build --release --bin lxr_backup --bin lxr_restore --manifest-path "$PROJECT_DIR/Cargo.toml" || exit 1
  BIN="$PROJECT_DIR/target/release"
fi

WORK="$(mktemp -d)"
trap 'chmod -R u+w "${WORK}" 2>/dev/null; rm -rf "${WORK}"' EXIT
cd "${WORK}"

MYID=clitest
NFAIL=0

# expect <exit code> <description> <command...>
expect() {
  local want=$1 desc=$2; shift 2
  "$@" > out.log 2>&1
  local got=$?
  if [ "$got" -eq "$want" ]; then
    echo "  ok    ${desc}"
  else
    echo "  FAIL  ${desc}: exit code ${got}, expected ${want}"
    sed 's/^/        | /' out.log
    NFAIL=$((NFAIL + 1))
  fi
}

check() {
  local desc=$1; shift
  if "$@"; then echo "  ok    ${desc}"; else echo "  FAIL  ${desc}"; NFAIL=$((NFAIL + 1)); fi
}

backup()  { "${BIN}/lxr_backup" -x chunks -d db -n 16 -i "${MYID}" "$@"; }
restore() { "${BIN}/lxr_restore" -x chunks -d db -i "${MYID}" "$@"; }
nchunkfiles() { find chunks -name '*.lxr' | wc -l | tr -d ' '; }
relfile_of() { grep -rl "\"fname\": \"$1\"" "db/${MYID}/relfiles"; }
nblocks() { grep -c '"blockid"' "$(relfile_of "$1")"; }

head -c 300000 /dev/urandom > a.bin
head -c 70000 /dev/urandom > b.bin

echo "lxr_backup"
expect 2 "missing -i is a usage error" "${BIN}/lxr_backup" -x chunks -d db a.bin
expect 2 "-n below 16 is a usage error" "${BIN}/lxr_backup" -x chunks -d db -n 8 -i "${MYID}" a.bin
expect 2 "-n above 256 is a usage error" "${BIN}/lxr_backup" -x chunks -d db -n 300 -i "${MYID}" a.bin
expect 2 "-n without a number is a usage error" "${BIN}/lxr_backup" -x chunks -d db -n abc -i "${MYID}" a.bin
expect 2 "an option without value is a usage error" "${BIN}/lxr_backup" a.bin -i
expect 2 "an unknown option is a usage error" "${BIN}/lxr_backup" -i "${MYID}" --frobnicate a.bin
check "usage errors create no chunks or database" test ! -e chunks -a ! -e db
expect 0 "--help" "${BIN}/lxr_backup" --help
expect 0 "backup of two files" backup a.bin b.bin
check "one assembly of 16 chunks" test "$(nchunkfiles)" -eq 16
check "chunk files have 262144 bytes" test -z "$(find chunks -name '*.lxr' ! -size 262144c)"
check "key files are private" test -z "$(find "db/${MYID}/relkeys" -type f -perm +077 2>/dev/null || find "db/${MYID}/relkeys" -type f -perm /077)"
expect 1 "backup of a missing file fails" backup missing.bin
touch stamp
head -c 50000 /dev/urandom > c.bin
expect 1 "backup with one missing file fails" backup c.bin missing.bin
mkdir -p o0 && expect 0 "the readable file was still backed up" restore -o o0 c.bin
expect 1 "backup of a directory as a file fails" backup o0

echo "lxr_restore"
expect 2 "missing -i is a usage error" "${BIN}/lxr_restore" -x chunks -d db -o o0 a.bin
mkdir -p o1
expect 0 "restore of two files" restore -o o1 a.bin b.bin
check "restored files are identical" cmp -s a.bin o1/a.bin
check "restored files are identical" cmp -s b.bin o1/b.bin
mkdir -p o2
expect 1 "restore with a different identifier finds no meta data" \
  "${BIN}/lxr_restore" -x chunks -d db -i other -o o2 a.bin
expect 1 "restore of an existing file fails" restore -o o1 a.bin
check "the existing file is untouched" cmp -s a.bin o1/a.bin
expect 2 "restore from a missing database" "${BIN}/lxr_restore" -x chunks -d nodb -i "${MYID}" -o o2 a.bin
check "no database is created" test ! -e nodb
expect 2 "restore into a missing output directory" restore -o nodir a.bin
expect 1 "a name with '..' is refused" restore -o o2 ../a.bin
expect 0 "-n is obsolete and ignored" restore -n 99 -o o2 b.bin
check "restore with -n is identical" cmp -s b.bin o2/b.bin
head -c 2000 /dev/urandom > abs.bin
expect 0 "backup of a file by its absolute name" backup "${WORK}/abs.bin"
mkdir -p o2a
expect 0 "restore by the absolute name" restore -o o2a "${WORK}/abs.bin"
check "an absolute name is restored below the output directory" cmp -s abs.bin "o2a${WORK}/abs.bin"

# c.bin lives alone in the last assembly (written after "stamp")
FIRST_C=$(find chunks -name '*.lxr' -newer stamp | sort | head -n1)
check "c.bin has its own assembly" test -n "${FIRST_C}"
cp "${FIRST_C}" saved.lxr
chmod u+w "${FIRST_C}"
printf '\x55' | dd of="${FIRST_C}" bs=1 seek=1000 conv=notrunc 2>/dev/null
mkdir -p o3
expect 1 "restore from a corrupted chunk fails" restore -o o3 c.bin
expect 1 "verify detects the corrupted chunk" restore --verify c.bin
check "no partial file after corrupted chunk" test ! -e o3/c.bin
rm -f "${FIRST_C}"
expect 1 "restore with a missing chunk fails" restore -o o3 c.bin
check "no partial file after missing chunk" test ! -e o3/c.bin
cp saved.lxr "${FIRST_C}"
expect 0 "restore after repairing the chunk" restore -o o3 c.bin
check "repaired restore is identical" cmp -s c.bin o3/c.bin
expect 0 "verify all files" restore --verify
restore -v --verify > verify.log 2>&1
check "verify covers all 4 files" grep -q "verified 4 of 4 files" verify.log

echo "containment"
mkdir -p sub outside o4
head -c 1000 /dev/urandom > sub/x.bin
expect 0 "backup of a file in a subdirectory" backup sub/x.bin
ln -s "${WORK}/outside" o4/sub
expect 1 "restore through a symlink out of the output directory is refused" restore -o o4 sub/x.bin
check "nothing was written outside" test -z "$(ls -A outside)"
mkdir -p o5
expect 0 "restore into a subdirectory" restore -o o5 sub/x.bin
check "restored file is identical" cmp -s sub/x.bin o5/sub/x.bin

echo "meta data errors"
KEYFILE=$(find "db/${MYID}/relkeys" -type f | head -n1)
chmod 644 "${KEYFILE}"
expect 1 "a key file readable by others is refused" restore --verify
chmod 600 "${KEYFILE}"
expect 0 "verify after fixing the permissions" restore --verify
if [ "$(id -u)" -ne 0 ]; then
  cp -R db db.ro && chmod -R a-w db.ro
  head -c 1000 /dev/urandom > d.bin
  expect 1 "backup into a read-only database fails" "${BIN}/lxr_backup" -x chunks -d db.ro -n 16 -i "${MYID}" d.bin
  chmod -R u+w db.ro
else
  echo "  skip  read-only database (running as root)"
fi

echo "incremental backup"
S="abcdefghjknpqrstuvwxyz0123456789"
for i in $(seq 1 2000); do echo "$S"; done > inc.small
cp inc.small inc.large
for i in $(seq 1 4000); do printf '%s' "$S"; done >> inc.large
inc_roundtrip() {
  local desc=$1 orig=$2
  rm -rf oi && mkdir oi
  expect 0 "${desc}: restore" restore -o oi inc.bin
  check "${desc}: restored file is identical" cmp -s "${orig}" oi/inc.bin
}
cp inc.small inc.bin
expect 0 "backup small file" backup inc.bin
N1=$(nblocks inc.bin)
inc_roundtrip "small" inc.small
cp inc.large inc.bin
NCHUNKS_BEFORE=$(nchunkfiles)
expect 0 "backup grown file" backup inc.bin
N2=$(nblocks inc.bin)
check "grown file has more blocks (${N1} -> ${N2})" test "${N2}" -gt "${N1}"
check "grown file writes one new assembly" test "$(nchunkfiles)" -eq $((NCHUNKS_BEFORE + 16))
inc_roundtrip "grown" inc.large
NCHUNKS_BEFORE=$(nchunkfiles)
expect 0 "backup unchanged file" backup inc.bin
check "unchanged file writes no new chunks" test "$(nchunkfiles)" -eq "${NCHUNKS_BEFORE}"
cp inc.small inc.bin
expect 0 "backup shrunk file" backup inc.bin
check "shrunk file has the original number of blocks" test "$(nblocks inc.bin)" -eq "${N1}"
inc_roundtrip "shrunk" inc.small
head -c 40000 /dev/urandom > e.bin
expect 0 "the same new file twice in one run" backup e.bin e.bin
mkdir -p o6
expect 0 "restore after a double backup" restore -o o6 e.bin
check "restored file is identical" cmp -s e.bin o6/e.bin
expect 0 "verify everything at the end" restore --verify

echo
if [ "${NFAIL}" -eq 0 ]; then
  echo "===== all command line tests passed ====="
else
  echo "===== ${NFAIL} command line test(s) failed ====="
  exit 1
fi

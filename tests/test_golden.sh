#!/usr/bin/env bash
# Golden test: interoperability with the reference implementation elykseer-ml
# (the formal specification, run from its Docker image).
#
#   ml backup  ->  rust lxr_restore  (and --verify)   checksums must match
#   rust backup ->  ml lxr_restore                    checksums must match
#
# The ml tools keep their meta data in an irmin git store, this implementation
# in JSON files with the same paths and content: the script moves the values
# between the two with git.
#
#   tests/test_golden.sh [directory with lxr_backup and lxr_restore]
#
# Without an argument the release binaries are built and used.
# LXR_ML_TAG selects the image tag (default: v0.10.3-1, elykseer-ml 0.10.3).

set -euo pipefail

LXR_ML_TAG="${LXR_ML_TAG:-v0.10.3-1}"
IMAGE="codieplusplus/elykseer-ml:${LXR_ML_TAG}"
ML_BIN=/home/coq/.local/bin

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
if [ $# -ge 1 ]; then
  BIN="$(cd "$1" && pwd)"
else
  cargo build --release --bin lxr_backup --bin lxr_restore --manifest-path "$PROJECT_DIR/Cargo.toml"
  BIN="$PROJECT_DIR/target/release"
fi
ls -l "${BIN}/lxr_backup" "${BIN}/lxr_restore"

docker image inspect "${IMAGE}" > /dev/null 2>&1 || docker pull "${IMAGE}"

# the work directory is mounted into the container: it must be in a directory
# the Docker daemon can share (on macOS not $TMPDIR or /tmp), so use target/
GOLDEN_TMP="${LXR_GOLDEN_TMP:-${PROJECT_DIR}/target}"
mkdir -p "${GOLDEN_TMP}"
WORKDIR="$(mktemp -d "${GOLDEN_TMP}/golden_XXXXXX")"
trap 'rm -rf "${WORKDIR}"' EXIT

# sizes in bytes: tiny, one block, several blocks, more than one assembly of 16 chunks
SIZES=(120 32768 1048576 5242977)
NCHUNKS_LIST=(16 29)

NFAIL=0
fail() { echo "  FAIL  $*"; NFAIL=$((NFAIL + 1)); }
pass() { echo "  ok    $*"; }

sha256() {
  if command -v sha256sum > /dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi | awk '{print $1}'
}

# the ml tools run as the calling user, so that files in the mounted work
# directory belong to us
ml() {
  docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp \
    -v "${WORKDIR}:/work" -w /work/src "${IMAGE}" "${ML_BIN}/$1" "${@:2}"
}

# irmin git store (bare, in <dir>/.git) -> JSON files <dir2>/<path>.json
git_to_json() {
  local from=$1 to=$2 p
  git --git-dir="${from}/.git" ls-tree -r --name-only main | while read -r p; do
    mkdir -p "${to}/$(dirname "${p}")"
    git --git-dir="${from}/.git" show "main:${p}" > "${to}/${p}.json"
  done
  chmod -R go-rwx "${to}"
}

# JSON files <dir>/<path>.json -> irmin git store <dir2>/.git
json_to_git() {
  local from=$1 to=$2 p
  mkdir -p "${to}.tree"
  (cd "${from}" && find . -name '*.json' | while read -r p; do
    p=${p#./}
    mkdir -p "${to}.tree/$(dirname "${p}")"
    cp "${p}" "${to}.tree/${p%.json}"
  done)
  git -C "${to}.tree" init -q -b main
  git -C "${to}.tree" add -A
  git -C "${to}.tree" -c user.name=golden -c user.email=golden@example.invalid commit -q -m "import"
  mkdir -p "${to}"
  mv "${to}.tree/.git" "${to}/.git"
  rm -rf "${to}.tree"
}

# compare the restored files below $1 with the checksums in $2
check_restored() {
  local desc=$1 outdir=$2 sums=$3 f sum
  while read -r sum f; do
    if [ -f "${outdir}/${f}" ] && [ "$(sha256 "${outdir}/${f}")" = "${sum}" ]; then
      pass "${desc}: ${f}"
    else
      fail "${desc}: ${f}"
    fi
  done < "${sums}"
}

for nc in "${NCHUNKS_LIST[@]}"; do
  T="${WORKDIR}/n${nc}"
  mkdir -p "${T}"
  # random data; the files are backed up by their relative names
  SRC="${WORKDIR}/src"
  rm -rf "${SRC}"
  mkdir -p "${SRC}"
  : > "${T}/sums"
  FILES=()
  for sz in "${SIZES[@]}"; do
    f="rnd_${sz}.bin"
    head -c "${sz}" /dev/urandom > "${SRC}/${f}"
    echo "$(sha256 "${SRC}/${f}") ${f}" >> "${T}/sums"
    FILES+=("${f}")
  done

  echo "ml backup -> rust restore (nchunks=${nc})"
  ID="mlrs${nc}"
  ml lxr_backup -x "/work/n${nc}/ml.chunks" -d "/work/n${nc}/ml.db" -n "${nc}" -i "${ID}" "${FILES[@]}" > "${T}/ml_backup.log" 2>&1 \
    || { cat "${T}/ml_backup.log"; fail "ml lxr_backup"; continue; }
  git_to_json "${T}/ml.db" "${T}/rs.db"
  mkdir -p "${T}/out1"
  if "${BIN}/lxr_restore" -x "${T}/ml.chunks" -d "${T}/rs.db" -i "${ID}" -o "${T}/out1" "${FILES[@]}" > "${T}/rs_restore.log" 2>&1; then
    check_restored "rust restores ml backup" "${T}/out1" "${T}/sums"
  else
    cat "${T}/rs_restore.log"; fail "rust lxr_restore of the ml backup"
  fi
  if "${BIN}/lxr_restore" --verify -x "${T}/ml.chunks" -d "${T}/rs.db" -i "${ID}" > "${T}/rs_verify.log" 2>&1; then
    pass "rust --verify of the ml backup"
  else
    cat "${T}/rs_verify.log"; fail "rust lxr_restore --verify of the ml backup"
  fi

  echo "rust backup -> ml restore (nchunks=${nc})"
  ID="rsml${nc}"
  (cd "${SRC}" && "${BIN}/lxr_backup" -x "${T}/rs.chunks" -d "${T}/rs2.db" -n "${nc}" -i "${ID}" "${FILES[@]}") > "${T}/rs_backup.log" 2>&1 \
    || { cat "${T}/rs_backup.log"; fail "rust lxr_backup"; continue; }
  json_to_git "${T}/rs2.db" "${T}/ml2.db"
  mkdir -p "${T}/out2"
  if ml lxr_restore -x "/work/n${nc}/rs.chunks" -d "/work/n${nc}/ml2.db" -i "${ID}" -o "/work/n${nc}/out2" "${FILES[@]}" > "${T}/ml_restore.log" 2>&1; then
    check_restored "ml restores rust backup" "${T}/out2" "${T}/sums"
  else
    cat "${T}/ml_restore.log"; fail "ml lxr_restore of the rust backup"
  fi
  if ml lxr_restore --verify -x "/work/n${nc}/rs.chunks" -d "/work/n${nc}/ml2.db" -i "${ID}" > "${T}/ml_verify.log" 2>&1; then
    pass "ml --verify of the rust backup"
  else
    cat "${T}/ml_verify.log"; fail "ml lxr_restore --verify of the rust backup"
  fi
done

echo
if [ "${NFAIL}" -eq 0 ]; then
  echo "===== golden test passed against ${IMAGE} ====="
else
  echo "===== ${NFAIL} golden check(s) failed against ${IMAGE} ====="
  exit 1
fi

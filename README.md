# eLyKseeR in _Rust_

[![CI](https://github.com/eLyKseeR/elykseer-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/eLyKseeR/elykseer-rs/actions/workflows/ci.yml)
[![License: GPL v3 or later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)

_eLyKseeR_ is a cryptographic data archive: it keeps backups confidential and tamper-evident on storage you do not trust.

This repository is the Rust implementation. It follows the formal specification in Rocq (former Coq), [elykseer-ml](https://github.com/eLyKseeR/elykseer-ml) **v0.10.3**, and the [model](https://github.com/eLyKseeR/modelling) of its data and processes. It provides

- the library `elykseer-lib`
- `lxr_backup`: encrypt and back up files
- `lxr_restore`: decrypt and restore files

The version number is the version of the specification it follows. The project is pre-1.0: the archive format may still change between minor versions, see the [changelog](CHANGELOG.md).

## Quickstart

### Run programs from pre-built images

The image [ghcr.io/elykseer/elykseer-rs](https://github.com/eLyKseeR/elykseer-rs/pkgs/container/elykseer-rs) contains `lxr_backup` (the entrypoint) and `lxr_restore`. It sees only the directories you mount; here one working directory is mounted at `/work`. The container runs as your user, so that the files it writes belong to you. The working directory must be one that Docker can share (on macOS below your home directory, not `$TMPDIR`).

preparations:
```sh
WORKDIR=$(mktemp -d "${HOME}/lxr_demo_XXXXXX")
mkdir -p "${WORKDIR}/meta" "${WORKDIR}/chunks" "${WORKDIR}/data" "${WORKDIR}/output"
head -c 8388608 /dev/urandom > "${WORKDIR}/data/file001.dat"
head -c 3145728 /dev/urandom > "${WORKDIR}/data/file002.dat"
MYID=$(head -c 16 /dev/urandom | od -An -tx1 | tr -d ' \n')
```

backup of the files in the directory `data`:
```sh
docker run --rm --user "$(id -u):$(id -g)" -v "${WORKDIR}:/work" ghcr.io/elykseer/elykseer-rs:latest -v -i "${MYID}" -x /work/chunks -d /work/meta -D /work/data
```

restore of one file into the directory `output`; the file is named as it was backed up:
```sh
docker run --rm --user "$(id -u):$(id -g)" -v "${WORKDIR}:/work" --entrypoint /usr/local/bin/lxr_restore ghcr.io/elykseer/elykseer-rs:latest -v -i "${MYID}" -x /work/chunks -d /work/meta -o /work/output /work/data/file001.dat
```

compare with the original (the restored file is below `output` under its full name):
```sh
cmp "${WORKDIR}/data/file001.dat" "${WORKDIR}/output/work/data/file001.dat" && echo EQUAL
```

### Building the programs from source
```sh
cargo build --release
PATH=$PWD/target/release:$PATH

MYID=$(head -c 16 /dev/urandom | od -An -tx1 | tr -d ' \n')
lxr_backup  -x ~/lxr.chunks -d ~/lxr.db -i $MYID myfile
mkdir -p /tmp/restored
lxr_restore -x ~/lxr.chunks -d ~/lxr.db -i $MYID -o /tmp/restored myfile
cmp myfile /tmp/restored/myfile
```

Keep the identifier (`-i`): without it neither the meta data nor the chunks are found.
The chunks (`-x`) can be stored anywhere; the database (`-d`) contains the keys and must be kept safe.

## What it protects against

- file data is split into blocks of at most 32 KiB, collected in assemblies of 16–256 chunks of 256 KiB, and each assembly is encrypted with AES-256-GCM under its own random key; the assembly id is authenticated, so chunks cannot be swapped or modified unnoticed
- the bytes of a file are striped over the chunks of an assembly, adjacent bytes end up in different chunk files
- chunk file names are hashes; they reveal neither file names nor sizes, only the total amount of data
- blocks are deduplicated, so unchanged data is not stored twice

It does **not** protect:

- the meta data database (`-d`): it holds the encryption keys in plain text and must stay on trusted storage; whoever has it and the chunks can read the backup, whoever loses it cannot. The key files are created readable by the owner only, and `lxr_restore` refuses key files that are accessible by group or others (`chmod 600`)
- availability: deleted or lost chunks make the affected files unrecoverable
- the machine running the backup, and data before it is encrypted

## Building

Rust 1.99 or later:

```sh
cargo build --release
```

`shell.nix` provides the same toolchain as the CI (`nix-shell`).

## Usage

### lxr_backup

```
lxr_backup -i <id> [options] [-D <dir> [-R]] [<file> ...]
  -i <id>       own identifier (required)
  -x <path>     output path for encrypted chunks (default: lxr)
  -d <path>     database path (default: $XDG_DATA_HOME/elykseer or ~/.local/share/elykseer)
  -n <int>      number of chunks 16-256 per assembly (default: 16)
  -m <octal>    permissions of chunk files (default: 600)
  -D <dir>      directory to backup
  -R            recursively backup directory
  -y            dry run: print the metadata instead of writing it
  -v            verbose output
```

- a file is identified by its name exactly as given on the command line (`./a` and `a` are different files); restore uses the same name
- a file whose content is unchanged since its last backup is skipped; of a changed file only the changed blocks are stored again
- symbolic links in directories are skipped
- the meta data is written at the end of a run; after an interrupted run, run the backup again

### lxr_restore

```
lxr_restore -i <id> [options] <file> ...
lxr_restore -i <id> --verify [options] [<file> ...]
  -i <id>       own identifier (required)
  -x <path>     path for encrypted chunks (default: lxr)
  -d <path>     database path (default: $XDG_DATA_HOME/elykseer or ~/.local/share/elykseer)
  -o <path>     output path for restored files (default: $TMPDIR)
  --verify      restore into a temporary directory, compare each file with its
                checksum from the metadata, and remove it; without file names:
                all files of the identifier
  -v            verbose output
```

- files are restored below the output directory under the name they were backed up with: a leading `/` is dropped (like tar), names containing `..` and paths that leave the output directory through symbolic links are refused
- an existing file is never overwritten
- a file that cannot be restored completely is removed again and reported
- the number of chunks per assembly is read from the key information; `-n` is accepted for compatibility and ignored
- owner, permissions and modification time are recorded in the meta data but not applied to restored files

### Exit codes

| tool | 0 | 1 | 2 |
|---|---|---|---|
| `lxr_backup` | all files backed up | a file could not be read or backed up, meta data could not be written, or a block refers to an assembly without key | usage error |
| `lxr_restore` | all files restored (or verified) completely | a file has no meta data, already exists, leaves `-o`, could not be restored completely (it is removed), or fails `--verify` | usage error, no database, no output directory |

## What is written to disk

| item | path | content |
|---|---|---|
| chunk | `<chunks>/<c63c64>/<c61c62>/<chunk id>.lxr` | 262144 bytes of an encrypted assembly; the chunk id is the SHA3-256 of `my_id ^ cid ^ aid`, the subdirectories are its last four hex characters |
| key information | `<db>/<my_id>/relkeys/<aid[4..5]>/<aid>.json` | `{ "version": {...}, "keys": { "ivec", "pkey", "localnchunks" } }` |
| file information | `<db>/<my_id>/relfiles/<fhash[4..5]>/<fhash>.json` | `{ "version": {...}, "fileinformation": {...}, "blocks": [ { "blockid", "bchecksum", "blocksize", "filepos", "blockaid", "blockapos" }, ... ] }` |

All values in the JSON documents are strings. Directories below `<db>/<my_id>` are created with mode 0700, the files in them with mode 0600. Chunk files are 0600 unless `-m` says otherwise.

The assembly layout, the encryption parameters and the identifiers are those of elykseer-ml and are described in its [format documentation](https://github.com/eLyKseeR/elykseer-ml/blob/main/doc/08_Format.md) and in [keys and nonces](https://github.com/eLyKseeR/elykseer-ml/blob/main/doc/07_Keys_and_Nonces.md).

## Compatibility with elykseer-ml

Chunks are interchangeable with elykseer-ml 0.10.1 and later: an assembly written by one implementation is decrypted by the other, given its key information. This is checked in both directions against the elykseer-ml binaries.

The same in both implementations:

- assembly layout (striping, random header, the rows reserved for the GCM tag), AES-256-GCM with the assembly id as additional authenticated data, nonce = first 96 bits of `ivec`
- `aid`, chunk id, chunk path, `fhash`, `bchecksum`, `fchecksum`
- the JSON documents for key and file information, and their paths

Different:

| | elykseer-ml | elykseer-rs |
|---|---|---|
| meta data database | [irmin](https://irmin.org) git store | plain JSON files, same paths with the suffix `.json` |
| `-i` | optional, default with a warning | required |
| `-x`, `-d` | required | optional, defaults see above |
| file permissions | umask | private (0600/0700), `-m` for chunks |
| key files readable by others | accepted | refused |

To read an elykseer-ml archive, export the JSON values from its git store into files (and make them private):

```sh
git --git-dir=$MLDB/.git ls-tree -r --name-only main | while read -r p; do
  mkdir -p "$RSDB/$(dirname "$p")"
  git --git-dir=$MLDB/.git show "main:$p" > "$RSDB/$p.json"
done
chmod -R go-rwx "$RSDB"
```

Not implemented:

- distribution of chunks to sinks (`lxr_distribute`, `Distribution` in the specification)
- the tools `lxr_relfiles`, `lxr_relkeys` and `lxr_chunks`
- snapshots (planned in the model, not in elykseer-ml either)
- empty files: a file without blocks gets no meta data and cannot be restored (the same in elykseer-ml)

## Library

The modules follow the specification, bottom-up:

| module | specification (`theories/`) | |
|---|---|---|
| `common::nchunks` | `Nchunks.v` | number of chunks per assembly, 16–256 |
| `common::configuration` | `Configuration.v` | |
| `common::conversion`, `common::utilities` | `Conversion.v`, `Utilities.v` | random identifiers, SHA3-256 |
| `common::cstdio` | `Cstdio.v` | file access, plain and encrypted buffers, AES-256-GCM |
| `common::filesupport` | `Filesupport.v` | file information |
| `common::tracer` | `Tracer.v` | |
| `common::store` | `Store.v` | stores for keys, blocks and file information |
| `assembly` | `Assembly.v` | assembly states: plain writable, plain full, encrypted |
| `environment` | `Environment.v` | writable and readable environment of an assembly |
| `assemblycache` | `AssemblyCache.v` | cache of readable assemblies, read and write queues |
| `processor` | `Processor.v` | backup and restore of files |
| `common::metadata`, `common::keys`, `common::fsutils` | (OCaml code of elykseer-ml) | JSON meta data, key types, file permissions |

Types: Coq `N` is `u64`, `nat` is `usize`, `string` is `String`; `positive` is `u32` for block ids and chunk counts and `NonZeroU64` in the conversion functions.

## Testing

```sh
cargo test
tests/test_cli.sh
tests/roundtrip.sh
tests/test_golden.sh
```

- `cargo test`: unit tests of all modules
- `tests/test_cli.sh`: exit codes, corrupted and missing chunks, containment below the output directory, incremental backup, `--verify`
- `tests/roundtrip.sh`: backup and restore of files of different sizes of random data, with different numbers of chunks per assembly
- `tests/test_golden.sh`: interoperability with elykseer-ml, run from its Docker image (`LXR_ML_TAG`, default `v0.10.3-1`; needs Docker): ml backup → `lxr_restore` and `--verify` here, and `lxr_backup` here → ml restore and `--verify`, checked by SHA-256

Coverage: `cargo llvm-cov --html` writes the report to `target/llvm-cov/html/index.html`.

The same checks run in CI on GitHub (`.github/workflows/`) and on Forgejo (`.forgejo/workflows/`, in a prebuilt image with the pinned toolchain, see `.forgejo/images/ci/`). Dependencies are audited with `cargo deny check` ([deny.toml](deny.toml)).

## License

[GNU General Public License v3 or later](LICENSE)

## Contributing

Contributions are welcome! Before submitting your first pull request, please review our
[Contributor License Agreement](CLA.md) (CLA). The CLA allows us to maintain _eLyKseeR_ under a
dual-licensing model (GPLv3 for open source use, with commercial licenses available for proprietary
applications). It is the same for all eLyKseeR repositories. See [CONTRIBUTING.md](CONTRIBUTING.md) for how to sign.

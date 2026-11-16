# Changelog

The version number is the version of the specification ([elykseer-ml](https://github.com/eLyKseeR/elykseer-ml)) that this implementation follows.

## [0.10.3] - unreleased

Aligned with elykseer-ml 0.10.3. Chunks are now interchangeable with elykseer-ml 0.10.1 and later.

### Incompatible

- archives written by 0.10.0 and earlier cannot be read: the data layout in an assembly, the chunk names and the file checksum changed. Restore with the old version and back up again into a new database and chunk directory

### Change

- data is striped over the chunks of an assembly: the byte at logical position `i` is stored in chunk `i mod n` at offset `i div n`
- the last 16 rows of an assembly (16 × nchunks bytes) are reserved for the GCM tag
- chunk id = SHA3-256 of `my_id ^ cid ^ aid`; chunk path `<chunks>/<c63c64>/<c61c62>/<chunk id>.lxr`
- `fchecksum` is the SHA3-256 of the file content
- the size of an assembly is read from its key information (`localnchunks`) on restore; `lxr_restore -n` is obsolete and ignored
- block checksums are verified when a block is read from an assembly
- block ids are assigned by file position when the meta data is written
- "lxr_backup": unchanged files and blocks are deduplicated against the meta data of earlier runs
- "lxr_backup": exit code 1 if a file could not be read or backed up, if meta data could not be written, or if a block refers to an assembly without key; `-n` outside 16–256 is a usage error (it was silently clamped)
- "lxr_restore": exit code 1 if a file has no meta data, already exists, or could not be restored completely; an incomplete file is removed
- "lxr_restore --verify": restores into a temporary directory and compares each file with its checksum from the meta data; without file names all files of the identifier are checked
- both tools: options without value, unknown options and invalid numbers are usage errors (exit code 2); `--help`, `--version`
- symbolic links in directories are skipped by `lxr_backup -D`
- `Tracer` log functions are `Send + Sync`

### Fix

- finalising an assembly succeeded when only some of its chunks could be written
- a file that shrank while it was backed up was padded with zeros; it now fails
- `lxr_restore` refuses targets that leave the output directory through symbolic links; restored files are created exclusively
- reading and writing files handled partial reads and writes as end of file

### Removed

- unused modules `common::filectrl`, `common::os` and `fsutils::FsUtils`; `Processor::request_read` and `Processor::run_read_requests` (not in the specification)

## [0.10.0] - 2026-10-12

### Security

- the assembly id (aid) is bound to the encryption as additional authenticated data (AAD)
- SHA3-256 for `fhash`, `aid` and block checksums; random numbers from the operating system
- key and meta data files are written with mode 0600, their directories with 0700; chunk files 0600 by default (`lxr_backup -m`); key files accessible by group or others are refused
- identifiers read from meta data are validated before they are used in paths; restore refuses names with `..`
- key material is zeroized and never printed

### Change

- the default database path is `$XDG_DATA_HOME/elykseer` or `~/.local/share/elykseer`; `-i` is required
- meta data is read and written with serde

### Incompatible

- archives written by 0.9.x cannot be read

## [0.9.16] - 2026-04-02

- first version: transcription of the modules Assembly, AssemblyCache, Environment and Processor, `lxr_backup` and `lxr_restore`
- AES-256-GCM encryption of assemblies
- key and file meta data as JSON files (`relkeys`, `relfiles`)

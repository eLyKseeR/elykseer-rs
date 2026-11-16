use std::path::Path;

use crate::common::configuration::Configuration;
use crate::common::cstdio::{BufferEncrypted, BufferPlain};
use crate::common::fsutils::{create_dir_all_mode, dir_mode_for, write_new_file_mode};
use crate::common::keys::{Key128, Key256};
use crate::common::nchunks::Nchunks;
use crate::common::tracer::LogLevel;
use crate::common::utilities::{is_hex_id, sha3_256};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const CHUNKWIDTH: u64 = 256;
pub const CHUNKLENGTH: u64 = 1024;
pub const CHUNKSIZE: u64 = CHUNKWIDTH * CHUNKLENGTH; // 262144
pub fn assemblysize(n: &Nchunks) -> u64 {
    CHUNKSIZE * n.to_n()
}

// ---------------------------------------------------------------------------
// Utility helpers  (Coq: Utilities)
// ---------------------------------------------------------------------------

/// Coq: Utilities.rnd256 — SHA3-256 of `my_id` and 256 random bits.
fn rnd256(seed: &str) -> String {
    crate::common::utilities::rnd256(seed)
}

/// Coq: Utilities.make_list n  →  [1..n] as list of positive
fn make_list(n: &Nchunks) -> Vec<u64> {
    (1..=n.to_n()).collect()
}

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

/// Assembly identifier (Coq: aid_t := string)
pub type AidT = String;

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AssemblyInformation {
    pub nchunks: Nchunks,
    pub aid: AidT,
    pub apos: u64,
}

#[derive(Debug, Clone)]
pub struct KeyInformation {
    pub ivec: Key128,
    pub pkey: Key256,
    pub localnchunks: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockInformation {
    pub blockid: u32,
    pub bchecksum: String,
    pub blocksize: u64,
    pub filepos: u64,
    pub blockaid: AidT,
    pub blockapos: u64,
}

// ---------------------------------------------------------------------------
// mkaid
// ---------------------------------------------------------------------------

pub fn mkaid(c: &Configuration) -> AidT {
    rnd256(&c.my_id)
}

// ---------------------------------------------------------------------------
// set_apos (private helper)
// ---------------------------------------------------------------------------

fn set_apos(a: &AssemblyInformation, apos: u64) -> AssemblyInformation {
    AssemblyInformation {
        nchunks: a.nchunks,
        aid: a.aid.clone(),
        apos,
    }
}

// ---------------------------------------------------------------------------
// AssemblyPlainWritable
// ---------------------------------------------------------------------------

pub mod assembly_plain_writable {
    use super::*;
    use crate::common::cstdio;

    pub fn create(c: &Configuration) -> (AssemblyInformation, BufferPlain) {
        let chunks = c.config_nchunks;
        let mut b = BufferPlain::buffer_create(CHUNKSIZE * chunks.to_n());
        // The header is padding inside the encrypted region, it protects
        // nothing; if no random bytes are available it stays zero.
        let rb = cstdio::ranbuf128().unwrap_or_else(|e| {
            c.trace
                .log(LogLevel::Warning, &format!("random assembly header: {}", e));
            BufferPlain::buffer_create(128)
        });
        // header bytes: n_chunks - tag_len, at most the 128 random bytes
        // available; the tail of tag_len bytes holds the GCM tag
        let h = (chunks.to_n() - cstdio::TAG_SIZE as u64).min(128);
        let nb = BufferPlain::copy_sz_pos(&rb, 0, h, &mut b, 0);
        (
            AssemblyInformation {
                nchunks: chunks,
                aid: mkaid(c),
                apos: nb,
            },
            b,
        )
    }

    pub fn buffer_len(b: &BufferPlain) -> u64 {
        b.buffer_len()
    }

    pub fn calc_checksum(_b: &BufferPlain) -> String {
        "<>".to_string()
    }
}

// ---------------------------------------------------------------------------
// AssemblyEncrypted
// ---------------------------------------------------------------------------

pub mod assembly_encrypted {
    use super::*;

    pub fn create(c: &Configuration) -> (AssemblyInformation, BufferEncrypted) {
        let chunks = c.config_nchunks;
        let b = BufferEncrypted::buffer_create(CHUNKSIZE * chunks.to_n());
        (
            AssemblyInformation {
                nchunks: chunks,
                aid: mkaid(c),
                apos: 0,
            },
            b,
        )
    }

    pub fn buffer_len(b: &BufferEncrypted) -> u64 {
        b.buffer_len()
    }

    pub fn calc_checksum(b: &BufferEncrypted) -> String {
        b.calc_checksum()
    }
}

// ---------------------------------------------------------------------------
// AssemblyPlainFull
// ---------------------------------------------------------------------------

pub mod assembly_plain_full {
    use super::*;

    pub fn create(c: &Configuration) -> (AssemblyInformation, BufferPlain) {
        let chunks = c.config_nchunks;
        let sz = CHUNKSIZE * chunks.to_n();
        let b = BufferPlain::buffer_create(sz);
        (
            AssemblyInformation {
                nchunks: chunks,
                aid: mkaid(c),
                apos: sz,
            },
            b,
        )
    }

    pub fn buffer_len(b: &BufferPlain) -> u64 {
        b.buffer_len()
    }

    pub fn calc_checksum(b: &BufferPlain) -> String {
        b.calc_checksum()
    }
}

// ---------------------------------------------------------------------------
// Code_Writeable: finish, backup
// ---------------------------------------------------------------------------

/// Coq: finish — convert a writable assembly into a full (read-only) assembly.
pub fn finish(a: AssemblyInformation, b: BufferPlain) -> (AssemblyInformation, BufferPlain) {
    (a, b)
}

/// Physical layout of an assembly: data is striped over the chunks. The
/// byte at logical position `i` lives in chunk `i mod n` at offset `i div n`
/// (elykseer-ml `elykseer-base/assembly.cxx`, `doc/08_Format.md`), so
/// adjacent bytes of a file end up in different chunks.
///
/// Calls `f(physical index, index into the block)` for the `sz` logical
/// positions starting at `pos` of an assembly of `alen` bytes. Returns
/// `false` without calling `f` if the range is empty, does not fit, or
/// `alen` is not a multiple of the chunk size.
fn striped(alen: u64, sz: u64, pos: u64, mut f: impl FnMut(usize, usize)) -> bool {
    let nchunks = alen / CHUNKSIZE;
    if sz < 1 || nchunks * CHUNKSIZE != alen || pos.checked_add(sz).is_none_or(|e| e > alen) {
        return false;
    }
    // split pos once, then step (cnum, cidx) instead of dividing per byte
    let mut cnum = pos % nchunks;
    let mut cidx = pos / nchunks;
    for idx in 0..sz as usize {
        f((cnum * CHUNKSIZE + cidx) as usize, idx);
        cnum += 1;
        if cnum == nchunks {
            cnum = 0;
            cidx += 1;
        }
    }
    true
}

/// Coq: assembly_add_content — copy the first `sz` bytes of `src` into the
/// assembly buffer `tgt` at logical position `pos`. Returns `sz`, or 0 if
/// the bytes do not fit.
pub fn assembly_add_content(src: &BufferPlain, sz: u64, pos: u64, tgt: &mut BufferPlain) -> u64 {
    if src.buffer_len() < sz {
        return 0;
    }
    let (s, alen) = (src.data(), tgt.buffer_len());
    let t = tgt.data_mut();
    if striped(alen, sz, pos, |phys, idx| t[phys] = s[idx]) {
        sz
    } else {
        0
    }
}

/// Coq: backup — write content into the assembly, returning updated info + block info.
pub fn backup(
    a: &AssemblyInformation,
    b: &mut BufferPlain,
    fpos: u64,
    content: &BufferPlain,
) -> (AssemblyInformation, BlockInformation) {
    let apos_n = a.apos;
    let bsz = content.buffer_len();
    let chksum = content.calc_checksum();
    let nwritten = assembly_add_content(content, bsz, apos_n, b);
    // the block ids of a file are assigned when its metadata is written
    // (metadata::consolidate_files)
    let bi = BlockInformation {
        blockid: 1,
        bchecksum: chksum,
        blocksize: nwritten,
        filepos: fpos,
        blockaid: a.aid.clone(),
        blockapos: apos_n,
    };
    let a_new = set_apos(a, apos_n + nwritten);
    (a_new, bi)
}

// ---------------------------------------------------------------------------
// Code_Readable: encrypt, restore
// ---------------------------------------------------------------------------

/// Coq: encrypt — encrypt a full plain assembly buffer with key info.
/// The aid (UTF-8 bytes of the hex string) is the AAD, as in elykseer-ml.
pub fn encrypt(
    a: &AssemblyInformation,
    b: &BufferPlain,
    ki: &KeyInformation,
) -> Option<(AssemblyInformation, BufferEncrypted)> {
    use crate::common::cstdio;
    let a_new = set_apos(a, assemblysize(&a.nchunks));
    let (_bsz, benc) = cstdio::encrypt(b, a.aid.as_bytes(), &ki.ivec, &ki.pkey);
    Some((a_new, benc))
}

/// Coq: assembly_get_content — read `sz` bytes at logical position `pos` of
/// the assembly buffer `src` into `tgt`. Returns `sz`, or 0 if the range is
/// outside the assembly or `tgt` is too small.
pub fn assembly_get_content(src: &BufferPlain, sz: u64, pos: u64, tgt: &mut BufferPlain) -> u64 {
    if tgt.buffer_len() < sz {
        return 0;
    }
    let s = src.data();
    let t = tgt.data_mut();
    if striped(src.buffer_len(), sz, pos, |phys, idx| t[idx] = s[phys]) {
        sz
    } else {
        0
    }
}

/// Coq: restore — extract a block from a full assembly, verifying checksum.
pub fn restore(b: &BufferPlain, bi: &BlockInformation) -> Option<BufferPlain> {
    let bsz = bi.blocksize;
    let mut buf = BufferPlain::buffer_create(bsz);
    let nw = assembly_get_content(b, bsz, bi.blockapos, &mut buf);
    if nw == bsz {
        let bcksum = buf.calc_checksum();
        if bcksum == bi.bchecksum {
            Some(buf)
        } else {
            None
        }
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Code_Encrypted: decrypt, recall, extract
// ---------------------------------------------------------------------------

/// Coq: decrypt — decrypt an encrypted assembly buffer with key info.
/// Fails if the chunks belong to an assembly with a different aid.
pub fn decrypt(
    a: &AssemblyInformation,
    b: &BufferEncrypted,
    ki: &KeyInformation,
) -> Option<(AssemblyInformation, BufferPlain)> {
    use crate::common::cstdio;
    let a_new = set_apos(a, 0);
    let (_bsz, bdec) = cstdio::decrypt(b, a.aid.as_bytes(), &ki.ivec, &ki.pkey)?;
    Some((a_new, bdec))
}

/// Coq: chunk_identifier — SHA3-256 (hex) of `my_id ^ cid ^ aid`
/// (elykseer-ml `theories/MakeML.v`).
pub fn chunk_identifier(c: &Configuration, aid: &str, cid: u64) -> String {
    sha3_256(&format!("{}{}{}", c.my_id, cid, aid))
}

/// Coq: chunk_identifier_path — `<path_chunks>/<c63c64>/<c61c62>/<chunk id>.lxr`:
/// the subdirectories are the last four hex characters of the chunk id.
pub fn chunk_identifier_path(c: &Configuration, aid: &str, cid: u64) -> String {
    let cident = chunk_identifier(c, aid, cid);
    format!(
        "{}/{}/{}/{}.lxr",
        c.path_chunks,
        &cident[62..],
        &cident[60..62],
        cident
    )
}

/// Coq: load_chunk_from_path
fn load_chunk_from_path(sfp: &str) -> Option<BufferEncrypted> {
    let path = Path::new(sfp);
    if !path.is_file() {
        return None;
    }
    let data = std::fs::read(path).ok()?;
    if data.len() as u64 != CHUNKSIZE {
        return None;
    }
    Some(BufferEncrypted::from_buffer(&data))
}

/// Coq: recall — read all chunks from disk and reassemble the encrypted buffer.
pub fn recall(
    c: &Configuration,
    a: &AssemblyInformation,
) -> Option<(AssemblyInformation, BufferEncrypted)> {
    let aid = &a.aid;
    if !is_hex_id(aid) {
        c.trace.log(
            LogLevel::Warning,
            &format!("invalid assembly id: {:?}", aid),
        );
        return None;
    }
    let cidlist = make_list(&a.nchunks);
    let mut b = BufferEncrypted::buffer_create(a.nchunks.to_n() * CHUNKSIZE);
    let blen = b.buffer_len();

    let nread = cidlist.iter().fold(0u64, |nread, &cid| {
        let cpath = chunk_identifier_path(c, aid, cid);
        match load_chunk_from_path(&cpath) {
            None => nread,
            Some(cb) => {
                let apos = CHUNKSIZE * (cid - 1);
                if apos + CHUNKSIZE <= blen {
                    nread + BufferEncrypted::copy_sz_pos(&cb, 0, CHUNKSIZE, &mut b, apos)
                } else {
                    nread
                }
            }
        }
    });

    let a_new = set_apos(a, nread);
    if nread == blen {
        Some((a_new, b))
    } else {
        None
    }
}

/// Coq: store_chunk_to_path
/// The chunk file is created with permission bits `mode`.
fn store_chunk_to_path(sfp: &str, sz: u64, pos: u64, b: &BufferEncrypted, mode: u32) -> u64 {
    let path = Path::new(sfp);
    if path.exists() {
        return 0;
    }
    if let Some(dir) = path.parent()
        && !dir.is_dir()
        && create_dir_all_mode(dir, dir_mode_for(mode)).is_err()
    {
        return 0;
    }
    let mut chunk = BufferEncrypted::buffer_create(sz);
    let copied = BufferEncrypted::copy_sz_pos(b, pos, sz, &mut chunk, 0);
    if copied == 0 {
        return 0;
    }
    match write_new_file_mode(path, chunk.data(), mode) {
        Ok(()) => copied,
        Err(_) => 0,
    }
}

/// Coq: extract — write each chunk of the encrypted assembly to its own file.
pub fn extract(c: &Configuration, a: &AssemblyInformation, b: &BufferEncrypted) -> u64 {
    let aid = &a.aid;
    let cidlist = make_list(&a.nchunks);

    cidlist.iter().fold(0u64, |nwritten, &cid| {
        let cpath = chunk_identifier_path(c, aid, cid);
        let apos = CHUNKSIZE * (cid - 1);
        nwritten + store_chunk_to_path(&cpath, CHUNKSIZE, apos, b, c.chunk_mode)
    })
}

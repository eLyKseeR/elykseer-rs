use std::io::{Read, Seek, SeekFrom, Write};

use aes_gcm::aead::AeadInPlace;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce, Tag};

use super::keys::{Key128, Key256};
use super::utilities::{fill_random, sha3_256_bytes};

// ---------------------------------------------------------------------------
// EncryptionState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionState {
    Plain,
    Encrypted,
}

// ---------------------------------------------------------------------------
// Mlcpp_Cstdio: file modes and file-pointer operations
// ---------------------------------------------------------------------------

pub const READ_MODE: &str = "rb";
pub const WRITE_MODE: &str = "wb";
pub const WRITE_NEW_MODE: &str = "wx";
pub const APPEND_MODE: &str = "ab";

pub struct Fptr {
    file: std::fs::File,
}

impl Fptr {
    /// Seek to a position, returning success.
    pub fn seek_to(&mut self, pos: u64) -> bool {
        self.file.seek(SeekFrom::Start(pos)).is_ok()
    }
}

/// Coq: fopen
pub fn fopen(path: &str, mode: &str) -> Option<Fptr> {
    let file = match mode {
        READ_MODE => std::fs::File::open(path).ok()?,
        WRITE_MODE => std::fs::File::create(path).ok()?,
        // "wx" — create exclusively (O_EXCL): fails if the path exists,
        // also if it is a dangling symlink
        WRITE_NEW_MODE => std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .ok()?,
        APPEND_MODE => std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .ok()?,
        _ => return None,
    };
    Some(Fptr { file })
}

/// Coq: fclose
pub fn fclose(_fp: Fptr) -> Option<()> {
    // File is closed on drop; we just consume ownership.
    Some(())
}

/// Coq: fflush
pub fn fflush(mut fp: Fptr) -> Option<Fptr> {
    fp.file.flush().ok()?;
    Some(fp)
}

/// Coq: fread — read up to `n` bytes, returning (count, raw buffer).
/// Like C `fread`, the count is below `n` only at the end of the file.
pub fn fread(fp: &mut Fptr, n: u64) -> Option<(u64, Vec<u8>)> {
    let mut buf = Vec::with_capacity(n as usize);
    let count = (&fp.file).take(n).read_to_end(&mut buf).ok()?;
    Some((count as u64, buf))
}

/// Coq: fwrite — write up to `n` bytes from `data`, returning count written.
pub fn fwrite(fp: &mut Fptr, n: u64, data: &[u8]) -> Option<u64> {
    let to_write = (n as usize).min(data.len());
    fp.file.write_all(&data[..to_write]).ok()?;
    Some(to_write as u64)
}

/// Coq: ftell
pub fn ftell(fp: &mut Fptr) -> Option<u64> {
    fp.file.stream_position().ok()
}

/// Coq: fseek
pub fn fseek(mut fp: Fptr, pos: u64) -> Option<Fptr> {
    fp.file.seek(SeekFrom::Start(pos)).ok()?;
    Some(fp)
}

// ---------------------------------------------------------------------------
// BufferPlain
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BufferPlain {
    data: Vec<u8>,
}

impl BufferPlain {
    pub const STATE: EncryptionState = EncryptionState::Plain;

    pub fn buffer_create(size: u64) -> Self {
        BufferPlain {
            data: vec![0u8; size as usize],
        }
    }

    pub fn buffer_len(&self) -> u64 {
        self.data.len() as u64
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn calc_checksum(&self) -> String {
        block_checksum(&self.data)
    }

    /// Copy `count` bytes from `src[src_pos..]` into `dst[dst_pos..]`.
    /// Returns the number of bytes actually copied.
    pub fn copy_sz_pos(
        src: &BufferPlain,
        src_pos: u64,
        count: u64,
        dst: &mut BufferPlain,
        dst_pos: u64,
    ) -> u64 {
        copy_within(&src.data, src_pos, count, &mut dst.data, dst_pos)
    }

    /// Coq: from_buffer (cstdio_buffer → buffer_t)
    pub fn from_buffer(raw: &[u8]) -> Self {
        BufferPlain { data: raw.to_vec() }
    }

    /// Coq: to_buffer (buffer_t → cstdio_buffer)
    pub fn to_buffer(&self) -> Vec<u8> {
        self.data.clone()
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

// ---------------------------------------------------------------------------
// BufferEncrypted
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BufferEncrypted {
    data: Vec<u8>,
}

impl BufferEncrypted {
    pub const STATE: EncryptionState = EncryptionState::Encrypted;

    pub fn buffer_create(size: u64) -> Self {
        BufferEncrypted {
            data: vec![0u8; size as usize],
        }
    }

    pub fn buffer_len(&self) -> u64 {
        self.data.len() as u64
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn calc_checksum(&self) -> String {
        block_checksum(&self.data)
    }

    pub fn copy_sz_pos(
        src: &BufferEncrypted,
        src_pos: u64,
        count: u64,
        dst: &mut BufferEncrypted,
        dst_pos: u64,
    ) -> u64 {
        copy_within(&src.data, src_pos, count, &mut dst.data, dst_pos)
    }

    /// Coq: from_buffer (cstdio_buffer → buffer_t)
    pub fn from_buffer(raw: &[u8]) -> Self {
        BufferEncrypted { data: raw.to_vec() }
    }

    /// Coq: to_buffer (buffer_t → cstdio_buffer)
    pub fn to_buffer(&self) -> Vec<u8> {
        self.data.clone()
    }

    /// Convert to a plain buffer (same bytes, different type wrapper).
    pub fn to_plain(&self) -> BufferPlain {
        BufferPlain {
            data: self.data.clone(),
        }
    }

    /// Convert from a plain buffer.
    pub fn from_plain(plain: &BufferPlain) -> Self {
        BufferEncrypted {
            data: plain.data.clone(),
        }
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

// ---------------------------------------------------------------------------
// encrypt / decrypt
// ---------------------------------------------------------------------------

/// AES-256-GCM auth tag size in bytes.
pub const TAG_SIZE: usize = 16;

/// Coq: encrypt bin aad iv pw → (N, BufferEncrypted)
///   let blen := BufferPlain.buffer_len bin in
///   cpp_encrypt_buffer bin (blen - 16) aad iv pw
///
/// Encrypts the first `blen - 16` bytes using AES-256-GCM with `pw` as the
/// 256-bit key, the first 12 bytes of `iv` as the nonce, and `aad` as
/// additional authenticated data (the assembly id, see `assembly::encrypt`).
/// The 16-byte authentication tag is stored in the last 16 bytes of the
/// output buffer, so the output size equals the input size.
pub fn encrypt(bin: &BufferPlain, aad: &[u8], iv: &Key128, pw: &Key256) -> (u64, BufferEncrypted) {
    let blen = bin.data.len();
    assert!(blen >= TAG_SIZE, "buffer too small for AES-GCM tag");

    let cipher = Aes256Gcm::new(pw.as_bytes().into());
    let nonce = Nonce::from_slice(&iv.as_bytes()[..12]);

    let mut ciphertext = bin.data[..blen - TAG_SIZE].to_vec();
    let tag = cipher
        .encrypt_in_place_detached(nonce, aad, &mut ciphertext)
        .expect("AES-256-GCM encryption should not fail");

    ciphertext.extend_from_slice(&tag);
    let len = ciphertext.len() as u64;
    (len, BufferEncrypted { data: ciphertext })
}

/// Coq: decrypt bin aad iv pw → (N, BufferPlain)
///
/// Decrypts a buffer produced by `encrypt`.  The last 16 bytes are the
/// AES-GCM authentication tag.  Returns `None` if the tag does not verify
/// (wrong key, wrong nonce, wrong `aad`, tampered data).
pub fn decrypt(
    bin: &BufferEncrypted,
    aad: &[u8],
    iv: &Key128,
    pw: &Key256,
) -> Option<(u64, BufferPlain)> {
    let data = &bin.data;
    let blen = data.len();
    if blen < TAG_SIZE {
        return None;
    }

    let cipher = Aes256Gcm::new(pw.as_bytes().into());
    let nonce = Nonce::from_slice(&iv.as_bytes()[..12]);
    let tag = Tag::from_slice(&data[blen - TAG_SIZE..]);

    let mut plaintext = data[..blen - TAG_SIZE].to_vec();
    cipher
        .decrypt_in_place_detached(nonce, aad, &mut plaintext, tag)
        .ok()?;

    // Pad back to original assembly size (the tag area is not real data).
    plaintext.resize(blen, 0);

    let len = plaintext.len() as u64;
    Some((len, BufferPlain { data: plaintext }))
}

// ---------------------------------------------------------------------------
// ranbuf128
// ---------------------------------------------------------------------------

/// Coq: ranbuf128 — generate 128 random bytes (OS CSPRNG) as a BufferPlain.
pub fn ranbuf128() -> Result<BufferPlain, String> {
    let mut buf = BufferPlain::buffer_create(128);
    fill_random(&mut buf.data)?;
    Ok(buf)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Block checksum: SHA3-256 (hex), as `bchecksum` in elykseer-ml. It is
/// used for deduplication, so it must be collision resistant.
fn block_checksum(data: &[u8]) -> String {
    sha3_256_bytes(data)
}

fn copy_within(src: &[u8], src_pos: u64, count: u64, dst: &mut [u8], dst_pos: u64) -> u64 {
    let sp = src_pos as usize;
    let dp = dst_pos as usize;
    let cnt = count as usize;

    let avail_src = src.len().saturating_sub(sp);
    let avail_dst = dst.len().saturating_sub(dp);
    let n = cnt.min(avail_src).min(avail_dst);

    dst[dp..dp + n].copy_from_slice(&src[sp..sp + n]);
    n as u64
}

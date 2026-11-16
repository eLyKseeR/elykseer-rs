use std::num::NonZeroU64;

use sha3::{Digest, Sha3_256};
use zeroize::Zeroize;

// ---------------------------------------------------------------------------
// make_list
// ---------------------------------------------------------------------------

/// Coq: make_list (n : positive) : list positive
/// Generates [1..n] as a Vec of NonZeroU64.
pub fn make_list(n: NonZeroU64) -> Vec<NonZeroU64> {
    (1..=n.get()).map(|x| NonZeroU64::new(x).unwrap()).collect()
}

// ---------------------------------------------------------------------------
// List manipulation
// ---------------------------------------------------------------------------

/// Coq: drop_list (n : nat) (ls : list A) : list A
/// Drops the first `n` elements from `ls`.
pub fn drop_list<A: Clone>(n: usize, ls: &[A]) -> Vec<A> {
    if n >= ls.len() {
        Vec::new()
    } else {
        ls[n..].to_vec()
    }
}

/// Coq: take_list (n : nat) (ls : list A) : list A
/// Takes the first `n` elements from `ls`.
pub fn take_list<A: Clone>(n: usize, ls: &[A]) -> Vec<A> {
    let end = n.min(ls.len());
    ls[..end].to_vec()
}

// ---------------------------------------------------------------------------
// Axioms: rnd, rnd256, sha3_256
// ---------------------------------------------------------------------------

/// Coq: Axiom rnd : N -> N.
/// Returns a random number from the OS CSPRNG (ignores argument).
/// Panics if the operating system cannot provide random bytes.
pub fn rnd(_n: u64) -> u64 {
    let mut b = [0u8; 8];
    if let Err(e) = fill_random(&mut b) {
        panic!("rnd: {}", e);
    }
    u64::from_le_bytes(b)
}

/// Fill `buf` with bytes from the operating system's CSPRNG (`getrandom`).
pub fn fill_random(buf: &mut [u8]) -> Result<(), String> {
    getrandom::getrandom(buf).map_err(|e| format!("cannot obtain random bytes: {}", e))
}

/// Coq: Axiom rnd256 : string -> string.
/// SHA3-256 (hex) of `seed` (the `my_id`) concatenated with the hex
/// representation of 256 random bits, as extracted in elykseer-ml
/// (`theories/MakeML.v`; `doc/08_Format.md`, identifier `aid`).
///
/// Panics if the operating system cannot provide random bytes: an
/// assembly id must never be predictable, so there is no fallback.
pub fn rnd256(seed: &str) -> String {
    let mut rnd = [0u8; 32];
    if let Err(e) = fill_random(&mut rnd) {
        panic!("rnd256: {}", e);
    }
    let mut input = format!("{}{}", seed, bytes_to_hex(&rnd));
    rnd.zeroize();
    let out = sha3_256(&input);
    input.zeroize();
    out
}

/// SHA3-256 of arbitrary bytes as a 64-char lowercase hex string.
pub fn sha3_256_bytes(input: &[u8]) -> String {
    bytes_to_hex(&Sha3_256::digest(input))
}

/// Coq: Axiom sha3_256 : string -> string.
/// SHA3-256 of a string as a 64-char lowercase hex string.
pub fn sha3_256(input: &str) -> String {
    sha3_256_bytes(input.as_bytes())
}

/// SHA3-256 (hex) of the content of a file, read in a streaming fashion.
/// This is the file checksum `fchecksum` of elykseer-ml.
pub fn sha3_256_file(path: &std::path::Path) -> std::io::Result<String> {
    let mut hasher = Sha3_256::new();
    std::io::copy(&mut std::fs::File::open(path)?, &mut hasher)?;
    Ok(bytes_to_hex(&hasher.finalize()))
}

/// File hash `fhash`: SHA3-256 (hex) of `fname ^ my_id`, as in elykseer-ml.
pub fn file_hash(fname: &str, my_id: &str) -> String {
    sha3_256(&format!("{}{}", fname, my_id))
}

/// True if `s` is a 64-char lowercase hex string, the format of `aid` and
/// `fhash`. Values read from metadata must pass this check before they are
/// used to build file system paths.
pub fn is_hex_id(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

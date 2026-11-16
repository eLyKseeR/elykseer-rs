use std::fmt;

use zeroize::Zeroize;

use super::utilities::fill_random;

// ---------------------------------------------------------------------------
// Key128 — 128-bit AES initialisation vector (16 bytes / 32 hex chars)
// ---------------------------------------------------------------------------

/// The bytes are zeroized on drop and `Debug` does not print them.
#[derive(Clone, PartialEq, Eq)]
pub struct Key128([u8; 16]);

// ---------------------------------------------------------------------------
// Key256 — 256-bit AES encryption key (32 bytes / 64 hex chars)
// ---------------------------------------------------------------------------

/// The bytes are zeroized on drop and `Debug` does not print them.
#[derive(Clone, PartialEq, Eq)]
pub struct Key256([u8; 32]);

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_to_bytes(s: &str, dst: &mut [u8]) -> Result<(), String> {
    if s.len() != dst.len() * 2 {
        return Err(format!(
            "expected {} hex chars, got {}",
            dst.len() * 2,
            s.len()
        ));
    }
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        dst[i] = (hi << 4) | lo;
    }
    Ok(())
}

fn hex_nibble(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(format!("invalid hex character: {}", b as char)),
    }
}

// ---------------------------------------------------------------------------
// Key128 impl
// ---------------------------------------------------------------------------

impl Key128 {
    /// Generate a cryptographically random 128-bit key from the OS CSPRNG.
    pub fn random() -> Result<Self, String> {
        let mut k = Key128([0u8; 16]);
        fill_random(&mut k.0)?;
        Ok(k)
    }

    /// Encode as a 32-character lowercase hex string.
    pub fn to_hex(&self) -> String {
        bytes_to_hex(&self.0)
    }

    /// Decode from a 32-character hex string.
    pub fn from_hex(s: &str) -> Result<Self, String> {
        let mut k = Key128([0u8; 16]);
        hex_to_bytes(s, &mut k.0)?;
        Ok(k)
    }

    /// Raw bytes.
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// Key256 impl
// ---------------------------------------------------------------------------

impl Key256 {
    /// Generate a cryptographically random 256-bit key from the OS CSPRNG.
    pub fn random() -> Result<Self, String> {
        let mut k = Key256([0u8; 32]);
        fill_random(&mut k.0)?;
        Ok(k)
    }

    /// Encode as a 64-character lowercase hex string.
    pub fn to_hex(&self) -> String {
        bytes_to_hex(&self.0)
    }

    /// Decode from a 64-character hex string.
    pub fn from_hex(s: &str) -> Result<Self, String> {
        let mut k = Key256([0u8; 32]);
        hex_to_bytes(s, &mut k.0)?;
        Ok(k)
    }

    /// Raw bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// Zeroize on drop, redacted Debug
// ---------------------------------------------------------------------------

impl Drop for Key128 {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl Drop for Key256 {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl zeroize::ZeroizeOnDrop for Key128 {}
impl zeroize::ZeroizeOnDrop for Key256 {}

impl fmt::Debug for Key128 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Key128(<redacted>)")
    }
}

impl fmt::Debug for Key256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Key256(<redacted>)")
    }
}

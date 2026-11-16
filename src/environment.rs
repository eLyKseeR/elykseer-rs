use std::sync::Arc;

use crate::assembly::{
    self, AidT, AssemblyInformation, BlockInformation, KeyInformation, assembly_plain_full,
    assembly_plain_writable, assemblysize,
};
use crate::common::configuration::Configuration;
use crate::common::conversion::{i2s, n2i};
use crate::common::cstdio::{BufferPlain, TAG_SIZE};
use crate::common::keys::{Key128, Key256};
use crate::common::nchunks::Nchunks;
use crate::common::tracer::LogLevel;

// ---------------------------------------------------------------------------
// Key generation axioms
// ---------------------------------------------------------------------------

/// Coq: cpp_mk_key256 — generate a cryptographically random 256-bit key.
pub fn mk_key256() -> Result<Key256, String> {
    Key256::random()
}

/// Coq: cpp_mk_key128 — generate a cryptographically random 128-bit key.
pub fn mk_key128() -> Result<Key128, String> {
    Key128::random()
}

/// Fresh key information for one assembly (never reused, see elykseer-ml
/// doc/07_Keys_and_Nonces.md).
fn mk_keyinformation(localnchunks: u32) -> Result<KeyInformation, String> {
    Ok(KeyInformation {
        pkey: mk_key256()?,
        ivec: mk_key128()?,
        localnchunks,
    })
}

// ---------------------------------------------------------------------------
// EnvironmentWritable
// ---------------------------------------------------------------------------

pub struct EnvironmentWritable {
    pub cur_assembly: AssemblyInformation,
    pub cur_buffer: BufferPlain,
    pub econfig: Arc<Configuration>,
}

impl EnvironmentWritable {
    /// Coq: initial_environment
    pub fn initial_environment(c: Arc<Configuration>) -> Self {
        let (a, b) = assembly_plain_writable::create(&c);
        EnvironmentWritable {
            cur_assembly: a,
            cur_buffer: b,
            econfig: c,
        }
    }

    /// Coq: recreate_assembly
    pub fn recreate_assembly(&self) -> Self {
        let (a, b) = assembly_plain_writable::create(&self.econfig);
        EnvironmentWritable {
            cur_assembly: a,
            cur_buffer: b,
            econfig: Arc::clone(&self.econfig),
        }
    }

    /// Coq: finalise_assembly
    pub fn finalise_assembly(&self) -> Option<(AidT, KeyInformation)> {
        let a0 = &self.cur_assembly;
        let apos = a0.apos;
        let tracer = &self.econfig.trace;

        tracer.conditional_trace(
            apos > 0,
            LogLevel::Info,
            Some(&format!(
                "finalising assembly {} with apos = {}",
                a0.aid,
                i2s(n2i(apos))
            )),
            || {
                let (a, b) = assembly::finish(a0.clone(), self.cur_buffer.clone());
                let ki = match mk_keyinformation(self.econfig.config_nchunks.to_positive()) {
                    Ok(ki) => ki,
                    Err(e) => {
                        tracer.log(
                            LogLevel::Error,
                            &format!("cannot generate key for assembly {}: {}", a.aid, e),
                        );
                        return None;
                    }
                };
                tracer.optional_trace(
                    assembly::encrypt(&a, &b, &ki),
                    LogLevel::Warning,
                    Some(&format!("failed to encrypt assembly: {}", a.aid)),
                    || None,
                    LogLevel::Info,
                    Some(&format!("encrypted assembly: {}", a.aid)),
                    |(a_enc, b_enc)| {
                        let n = assembly::extract(&self.econfig, &a_enc, &b_enc);
                        // every chunk must be written, else the assembly cannot be recalled
                        tracer.conditional_trace(
                            n == assemblysize(&a_enc.nchunks),
                            LogLevel::Info,
                            None,
                            || Some((a.aid.clone(), ki)),
                            LogLevel::Warning,
                            Some(&format!(
                                "failed to write all chunks of assembly {}: {} bytes written",
                                a.aid,
                                i2s(n2i(n))
                            )),
                            || None,
                        )
                    },
                )
            },
            LogLevel::Info,
            Some(&format!(
                "not finalising empty assembly {} with apos = {}",
                a0.aid,
                i2s(n2i(apos))
            )),
            || None,
        )
    }

    /// Coq: finalise_and_recreate_assembly
    pub fn finalise_and_recreate_assembly(
        &self,
    ) -> Option<(EnvironmentWritable, (AidT, KeyInformation))> {
        let ki = self.finalise_assembly()?;
        let new_env = self.recreate_assembly();
        Some((new_env, ki))
    }

    /// Coq: backup
    pub fn backup(
        self,
        _fp: &str,
        fpos: u64,
        content: &BufferPlain,
    ) -> (
        EnvironmentWritable,
        (BlockInformation, Option<(AidT, KeyInformation)>),
    ) {
        // data is striped over the chunks: logical position i is stored in
        // chunk (i mod n) at offset (i / n); the GCM tag occupies the last
        // tag_len bytes of the last chunk, hence the last tag_len rows
        // (tag_len * n logical bytes) are not available for data
        let nchunks = self.econfig.config_nchunks;
        let afree = assemblysize(&nchunks)
            .saturating_sub(self.cur_assembly.apos)
            .saturating_sub(TAG_SIZE as u64 * nchunks.to_n());
        let blen = content.buffer_len();

        let (ki, env) = if afree < blen {
            match self.finalise_and_recreate_assembly() {
                None => (None, self),
                Some((new_env, ki_pair)) => (Some(ki_pair), new_env),
            }
        } else {
            (None, self)
        };

        let mut buf = env.cur_buffer;
        let (a_new, bi) = assembly::backup(&env.cur_assembly, &mut buf, fpos, content);

        (
            EnvironmentWritable {
                cur_assembly: a_new,
                cur_buffer: buf,
                econfig: env.econfig,
            },
            (bi, ki),
        )
    }
}

// ---------------------------------------------------------------------------
// EnvironmentReadable
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct EnvironmentReadable {
    pub cur_assembly: AssemblyInformation,
    pub cur_buffer: BufferPlain,
    pub econfig: Arc<Configuration>,
}

impl EnvironmentReadable {
    /// Coq: initial_environment
    pub fn initial_environment(c: Arc<Configuration>) -> Self {
        let (a, b) = assembly_plain_full::create(&c);
        EnvironmentReadable {
            cur_assembly: a,
            cur_buffer: b,
            econfig: c,
        }
    }

    /// Coq: restore_assembly — recall an encrypted assembly from disk, decrypt it.
    /// The assembly size is the one stored with its key (`localnchunks`),
    /// not the one configured now.
    pub fn restore_assembly(
        c: &Arc<Configuration>,
        aid: &str,
        ki: &KeyInformation,
    ) -> Option<EnvironmentReadable> {
        let recall_info = AssemblyInformation {
            nchunks: Nchunks::from_positive(ki.localnchunks),
            aid: aid.to_string(),
            apos: 0,
        };

        let (a1, b1) = assembly::recall(c, &recall_info)?;
        let (a2, b2) = assembly::decrypt(&a1, &b1, ki)?;

        Some(EnvironmentReadable {
            cur_assembly: a2,
            cur_buffer: b2,
            econfig: Arc::clone(c),
        })
    }
}

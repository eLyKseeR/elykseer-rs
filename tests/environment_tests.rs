use std::sync::Arc;

use elykseer_lib::assembly::{CHUNKSIZE, KeyInformation, assemblysize, restore};
use elykseer_lib::common::configuration::Configuration;
use elykseer_lib::common::cstdio::BufferPlain;
use elykseer_lib::common::keys::{Key128, Key256};
use elykseer_lib::common::tracer::Tracer;
use elykseer_lib::environment::{EnvironmentReadable, EnvironmentWritable, mk_key128, mk_key256};
use elykseer_lib::nchunks::Nchunks;

fn test_config() -> Arc<Configuration> {
    Arc::new(Configuration::new(
        16,
        "/tmp/elykseer_env_test_chunks".to_string(),
        "/tmp/elykseer_env_test_db".to_string(),
        "env-test-id".to_string(),
        Tracer::null_tracer(),
    ))
}

fn cleanup_dir(path: &str) {
    let _ = std::fs::remove_dir_all(path);
}

// ===========================================================================
// Key generation
// ===========================================================================

#[cfg(test)]
mod key_generation_tests {
    use super::*;

    #[test]
    fn mk_key256_returns_64_hex_chars() {
        let k = mk_key256().unwrap();
        let h = k.to_hex();
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn mk_key128_returns_32_hex_chars() {
        let k = mk_key128().unwrap();
        let h = k.to_hex();
        assert_eq!(h.len(), 32);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn mk_key256_successive_calls_differ() {
        let k1 = mk_key256().unwrap();
        let k2 = mk_key256().unwrap();
        assert_ne!(k1, k2);
    }

    #[test]
    fn mk_key128_successive_calls_differ() {
        let k1 = mk_key128().unwrap();
        let k2 = mk_key128().unwrap();
        assert_ne!(k1, k2);
    }
}

// ===========================================================================
// EnvironmentWritable
// ===========================================================================

#[cfg(test)]
mod writable_tests {
    use super::*;

    #[test]
    fn initial_environment_creates_valid_assembly() {
        let c = test_config();
        let env = EnvironmentWritable::initial_environment(c.clone());

        assert_eq!(env.cur_assembly.nchunks.to_positive(), 16);
        assert_eq!(env.cur_assembly.aid.len(), 64);
        assert_eq!(
            env.cur_buffer.buffer_len(),
            CHUNKSIZE * env.cur_assembly.nchunks.to_n()
        );
    }

    #[test]
    fn initial_environment_apos_is_header_size() {
        let c = test_config();
        let env = EnvironmentWritable::initial_environment(c);
        // apos should be nchunks - 16 (header bytes copied from ranbuf128)
        // nchunks=16 → apos = 16 - 16 = 0
        assert!(env.cur_assembly.apos <= env.cur_assembly.nchunks.to_n());
    }

    #[test]
    fn recreate_assembly_produces_different_aid() {
        let c = test_config();
        let env1 = EnvironmentWritable::initial_environment(c);
        let env2 = env1.recreate_assembly();

        // mkaid incorporates time, so recreated assembly gets a different aid
        assert_ne!(env1.cur_assembly.aid, env2.cur_assembly.aid);
        // But both are valid
        assert_eq!(env2.cur_assembly.nchunks.to_positive(), 16);
    }

    #[test]
    fn recreate_assembly_shares_config() {
        let c = test_config();
        let env1 = EnvironmentWritable::initial_environment(c.clone());
        let env2 = env1.recreate_assembly();

        assert_eq!(env2.econfig.my_id, c.my_id);
        assert_eq!(env2.econfig.config_nchunks, c.config_nchunks);
    }

    #[test]
    fn finalise_assembly_on_empty_returns_none() {
        let c = Arc::new(Configuration::new(
            16,
            "/tmp/elykseer_env_test_fin_empty".to_string(),
            "/tmp/elykseer_env_test_db".to_string(),
            "fin-empty-test".to_string(),
            Tracer::null_tracer(),
        ));

        // Create env with apos = 0 (manually)
        let env = EnvironmentWritable {
            cur_assembly: elykseer_lib::assembly::AssemblyInformation {
                nchunks: Nchunks::new(16),
                aid: "test-aid".to_string(),
                apos: 0,
            },
            cur_buffer: BufferPlain::buffer_create(CHUNKSIZE * 16),
            econfig: c,
        };

        // With apos=0, finalise should take the false branch → None
        let result = env.finalise_assembly();
        assert!(result.is_none());
    }

    #[test]
    fn backup_small_content_no_finalise() {
        let c = test_config();
        let env = EnvironmentWritable::initial_environment(c);
        let initial_apos = env.cur_assembly.apos;

        let mut content = BufferPlain::buffer_create(128);
        for (i, b) in content.data_mut().iter_mut().enumerate() {
            *b = (i % 256) as u8;
        }

        let (env2, (bi, ki)) = env.backup("testfile.dat", 0, &content);

        // No finalisation should have occurred
        assert!(ki.is_none());
        assert_eq!(bi.blocksize, 128);
        assert_eq!(bi.blockapos, initial_apos);
        assert_eq!(bi.filepos, 0);
        assert_eq!(env2.cur_assembly.apos, initial_apos + 128);
    }

    #[test]
    fn backup_multiple_small_contents() {
        let c = test_config();
        let env = EnvironmentWritable::initial_environment(c);

        let content1 = BufferPlain::buffer_create(100);
        let (env, (bi1, ki1)) = env.backup("file1.dat", 0, &content1);
        assert!(ki1.is_none());
        assert_eq!(bi1.blocksize, 100);

        let content2 = BufferPlain::buffer_create(200);
        let (env, (bi2, ki2)) = env.backup("file2.dat", 100, &content2);
        assert!(ki2.is_none());
        assert_eq!(bi2.blocksize, 200);

        // Blocks should be contiguous
        assert_eq!(bi1.blockapos + bi1.blocksize, bi2.blockapos);

        let _env = env; // suppress unused warning
    }

    #[test]
    fn backup_large_content_triggers_finalise() {
        let chunk_dir = "/tmp/elykseer_env_test_backup_large";
        cleanup_dir(chunk_dir);

        let c = Arc::new(Configuration::new(
            16,
            chunk_dir.to_string(),
            "/tmp/elykseer_env_test_db".to_string(),
            "backup-large-test".to_string(),
            Tracer::stdout_tracer_debug(),
        ));

        let env = EnvironmentWritable::initial_environment(c);
        let nchunks = Nchunks::new(16);
        let asm_size = assemblysize(&nchunks);

        // Fill up most of the assembly
        let fill_size = asm_size - 32; // leave only 16 bytes free (< 16 + content)
        let fill_content = BufferPlain::buffer_create(fill_size);
        let (env, (_, _)) = env.backup("fill.dat", 0, &fill_content);

        // Now backup something that exceeds remaining space → triggers finalise
        let content = BufferPlain::buffer_create(64);
        let (env2, (bi, ki)) = env.backup("overflow.dat", 0, &content);

        // Should have finalised the previous assembly
        assert!(ki.is_some(), "should have triggered finalise_and_recreate");
        assert_eq!(bi.blocksize, 64);

        // The new assembly should be different
        let _env2 = env2;

        cleanup_dir(chunk_dir);
    }

    #[test]
    fn finalise_and_recreate_with_content() {
        let chunk_dir = "/tmp/elykseer_env_test_fin_recreate";
        cleanup_dir(chunk_dir);

        let c = Arc::new(Configuration::new(
            16,
            chunk_dir.to_string(),
            "/tmp/elykseer_env_test_db".to_string(),
            "fin-recreate-test".to_string(),
            Tracer::stdout_tracer_debug(),
        ));

        let env = EnvironmentWritable::initial_environment(c);

        // Write some content so apos > 0
        let content = BufferPlain::buffer_create(256);
        let (env, _) = env.backup("test.dat", 0, &content);

        // Now finalise and recreate
        let result = env.finalise_and_recreate_assembly();
        assert!(result.is_some());
        let (new_env, (aid, ki)) = result.unwrap();

        assert!(!aid.is_empty());
        assert_eq!(ki.ivec.to_hex().len(), 32);
        assert_eq!(ki.pkey.to_hex().len(), 64);
        assert_eq!(ki.localnchunks, 16);

        // New environment should have a fresh assembly
        assert_eq!(new_env.cur_assembly.nchunks.to_positive(), 16);

        cleanup_dir(chunk_dir);
    }
}

// ===========================================================================
// EnvironmentReadable
// ===========================================================================

#[cfg(test)]
mod readable_tests {
    use super::*;

    #[test]
    fn initial_environment_creates_valid_assembly() {
        let c = test_config();
        let env = EnvironmentReadable::initial_environment(c);

        assert_eq!(env.cur_assembly.nchunks.to_positive(), 16);
        let expected_sz = CHUNKSIZE * env.cur_assembly.nchunks.to_n();
        assert_eq!(env.cur_buffer.buffer_len(), expected_sz);
        // PlainFull sets apos = total size
        assert_eq!(env.cur_assembly.apos, expected_sz);
    }

    #[test]
    fn restore_assembly_fails_when_no_chunks() {
        let chunk_dir = "/tmp/elykseer_env_test_restore_missing";
        cleanup_dir(chunk_dir);

        let c = Arc::new(Configuration::new(
            16,
            chunk_dir.to_string(),
            "/tmp/elykseer_env_test_db".to_string(),
            "restore-missing-test".to_string(),
            Tracer::null_tracer(),
        ));

        let ki = KeyInformation {
            pkey: Key256::from_hex(&"0".repeat(64)).unwrap(),
            ivec: Key128::from_hex(&"0".repeat(32)).unwrap(),
            localnchunks: 16,
        };

        let result = EnvironmentReadable::restore_assembly(&c, "nonexistent-aid", &ki);
        assert!(result.is_none());

        cleanup_dir(chunk_dir);
    }
}

// ===========================================================================
// Full round-trip: write → finalise → restore → read back
// ===========================================================================

#[cfg(test)]
mod round_trip_tests {
    use super::*;

    #[test]
    fn write_finalise_restore_round_trip() {
        let chunk_dir = "/tmp/elykseer_env_test_roundtrip";
        cleanup_dir(chunk_dir);

        let c = Arc::new(Configuration::new(
            16,
            chunk_dir.to_string(),
            "/tmp/elykseer_env_test_db".to_string(),
            "roundtrip-test".to_string(),
            Tracer::stdout_tracer_debug(),
        ));

        // --- Write phase ---
        let env_w = EnvironmentWritable::initial_environment(Arc::clone(&c));

        let mut content = BufferPlain::buffer_create(512);
        for (i, b) in content.data_mut().iter_mut().enumerate() {
            *b = ((i * 7 + 3) % 256) as u8;
        }

        let (env_w, (bi, _)) = env_w.backup("original.dat", 100, &content);

        // Finalise
        let result = env_w.finalise_assembly();
        assert!(result.is_some(), "finalise should succeed with content");
        let (aid, ki) = result.unwrap();

        // --- Read phase ---
        let restored_env = EnvironmentReadable::restore_assembly(&c, &aid, &ki);
        assert!(
            restored_env.is_some(),
            "restore should succeed after finalise + extract"
        );
        let env_r = restored_env.unwrap();

        // Restore the block
        let restored = restore(&env_r.cur_buffer, &bi);
        assert!(restored.is_some(), "restore block should succeed");
        let restored_buf = restored.unwrap();
        assert_eq!(restored_buf.data(), content.data());

        cleanup_dir(chunk_dir);
    }
}

// ===========================================================================
// Alignment with the specification (elykseer-ml 0.10.3)
// ===========================================================================

#[cfg(test)]
mod spec_alignment_tests {
    use super::*;
    use elykseer_lib::assembly::{CHUNKSIZE, chunk_identifier_path, restore};
    use elykseer_lib::common::cstdio::TAG_SIZE;

    fn config(n: u32, name: &str) -> (Arc<Configuration>, String) {
        let dir = format!("/tmp/elykseer_env_test_{}", name);
        cleanup_dir(&dir);
        let c = Arc::new(Configuration::new(
            n,
            dir.clone(),
            "/tmp/elykseer_env_test_db".to_string(),
            "spec-id".to_string(),
            Tracer::null_tracer(),
        ));
        (c, dir)
    }

    fn pattern(len: u64, seed: u8) -> BufferPlain {
        let mut b = BufferPlain::buffer_create(len);
        for (i, x) in b.data_mut().iter_mut().enumerate() {
            *x = ((i as u64 * 31 + seed as u64) % 255) as u8 + 1;
        }
        b
    }

    /// The last tag_len rows (tag_len * n logical bytes) are reserved for
    /// the GCM tag: a block that fills the assembly to capacity must come
    /// back intact, and one more byte must go to the next assembly.
    #[test]
    fn full_assembly_keeps_its_last_block() {
        let (c, dir) = config(16, "full");
        let n = 16u64;
        let capacity = n * CHUNKSIZE - TAG_SIZE as u64 * n;

        let mut env = EnvironmentWritable::initial_environment(Arc::clone(&c));
        let aid = env.cur_assembly.aid.clone();
        let mut fpos = 0;
        while capacity - env.cur_assembly.apos >= 32768 {
            let (e, (_, ki)) = env.backup("f", fpos, &pattern(32768, 1));
            assert!(ki.is_none());
            env = e;
            fpos += 32768;
        }
        let rest = capacity - env.cur_assembly.apos;
        assert!(rest > 0);
        let last = pattern(rest, 7);
        let (env, (last_bi, ki)) = env.backup("f", fpos, &last);
        assert!(ki.is_none(), "the block fits exactly");
        assert_eq!(last_bi.blockaid, aid);
        assert_eq!(env.cur_assembly.apos, capacity);

        // one more byte does not fit: the assembly is finalised
        let (env, (bi, ki)) = env.backup("f", fpos + rest, &pattern(1, 9));
        let (fin_aid, ki) = ki.expect("full assembly is finalised");
        assert_eq!(fin_aid, aid);
        assert_ne!(bi.blockaid, aid);
        assert_ne!(env.cur_assembly.aid, aid);

        let env_r = EnvironmentReadable::restore_assembly(&c, &aid, &ki).expect("restore");
        let restored = restore(&env_r.cur_buffer, &last_bi).expect("last block is intact");
        assert_eq!(restored.data(), last.data());

        cleanup_dir(&dir);
    }

    /// The assembly size comes from the key (`localnchunks`), not from the
    /// number of chunks configured at restore time.
    #[test]
    fn restore_uses_localnchunks_of_the_key() {
        let (c17, dir) = config(17, "localn");
        let env = EnvironmentWritable::initial_environment(Arc::clone(&c17));
        let content = pattern(5000, 3);
        let (env, (bi, _)) = env.backup("f", 0, &content);
        let (aid, ki) = env.finalise_assembly().expect("finalise");
        assert_eq!(ki.localnchunks, 17);

        let c16 = Arc::new(Configuration::new(
            16,
            dir.clone(),
            "/tmp/elykseer_env_test_db".to_string(),
            "spec-id".to_string(),
            Tracer::null_tracer(),
        ));
        let env_r = EnvironmentReadable::restore_assembly(&c16, &aid, &ki).expect("restore");
        assert_eq!(env_r.cur_buffer.buffer_len(), 17 * CHUNKSIZE);
        assert_eq!(
            restore(&env_r.cur_buffer, &bi).expect("block").data(),
            content.data()
        );

        cleanup_dir(&dir);
    }

    /// Every chunk must be written, else the assembly cannot be recalled.
    #[test]
    fn finalise_fails_if_a_chunk_cannot_be_written() {
        let (c, dir) = config(16, "partial");
        let env = EnvironmentWritable::initial_environment(Arc::clone(&c));
        let (env, _) = env.backup("f", 0, &pattern(100, 5));

        // chunk 5 exists already: it is not overwritten
        let blocked = chunk_identifier_path(&c, &env.cur_assembly.aid, 5);
        std::fs::create_dir_all(std::path::Path::new(&blocked).parent().unwrap()).unwrap();
        std::fs::write(&blocked, b"occupied").unwrap();

        assert!(env.finalise_assembly().is_none());

        cleanup_dir(&dir);
    }

    #[test]
    fn two_finalised_assemblies_differ_in_aid_key_and_nonce() {
        let (c, dir) = config(16, "fresh");
        let finalise = || {
            let env = EnvironmentWritable::initial_environment(Arc::clone(&c));
            let (env, _) = env.backup("f", 0, &pattern(100, 5));
            env.finalise_assembly().expect("finalise")
        };
        let (aid1, ki1) = finalise();
        let (aid2, ki2) = finalise();
        assert_ne!(aid1, aid2);
        assert_ne!(ki1.pkey.to_hex(), ki2.pkey.to_hex());
        assert_ne!(ki1.ivec.to_hex(), ki2.ivec.to_hex());

        cleanup_dir(&dir);
    }
}

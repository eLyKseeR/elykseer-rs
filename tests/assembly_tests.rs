use elykseer_lib::assembly::*;
use elykseer_lib::common::configuration::Configuration;
use elykseer_lib::common::cstdio::{BufferEncrypted, BufferPlain};
use elykseer_lib::common::keys::{Key128, Key256};
use elykseer_lib::common::tracer::Tracer;
use elykseer_lib::nchunks::Nchunks;

fn test_config() -> Configuration {
    Configuration::new(
        16,
        "/tmp/elykseer_test_chunks".to_string(),
        "/tmp/elykseer_test_db".to_string(),
        "test-id-001".to_string(),
        Tracer::null_tracer(),
    )
}

#[cfg(test)]
mod constants_tests {
    use super::*;

    #[test]
    fn chunksize_is_256_times_1024() {
        assert_eq!(CHUNKSIZE, 256 * 1024);
        assert_eq!(CHUNKSIZE, 262144);
    }

    #[test]
    fn assemblysize_matches_chunksize_times_nchunks() {
        let n = Nchunks::new(16);
        assert_eq!(assemblysize(&n), CHUNKSIZE * 16);

        let n256 = Nchunks::new(256);
        assert_eq!(assemblysize(&n256), CHUNKSIZE * 256);
    }
}

#[cfg(test)]
mod buffer_plain_tests {
    use super::*;

    #[test]
    fn create_has_correct_length() {
        let b = BufferPlain::buffer_create(1024);
        assert_eq!(b.buffer_len(), 1024);
    }

    #[test]
    fn create_zero_initialized() {
        let b = BufferPlain::buffer_create(64);
        assert!(b.data().iter().all(|&x| x == 0));
    }

    #[test]
    fn copy_sz_pos_copies_correct_bytes() {
        let mut src = BufferPlain::buffer_create(16);
        src.data_mut()[0..4].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);

        let mut dst = BufferPlain::buffer_create(16);
        let copied = BufferPlain::copy_sz_pos(&src, 0, 4, &mut dst, 4);

        assert_eq!(copied, 4);
        assert_eq!(&dst.data()[4..8], &[0xAA, 0xBB, 0xCC, 0xDD]);
        // rest untouched
        assert_eq!(&dst.data()[0..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn copy_sz_pos_clips_to_buffer_bounds() {
        let src = BufferPlain::buffer_create(8);
        let mut dst = BufferPlain::buffer_create(4);
        // try to copy 8 bytes into 4-byte buffer
        let copied = BufferPlain::copy_sz_pos(&src, 0, 8, &mut dst, 0);
        assert_eq!(copied, 4);
    }

    #[test]
    fn checksum_deterministic() {
        let b = BufferPlain::buffer_create(32);
        let c1 = b.calc_checksum();
        let c2 = b.calc_checksum();
        assert_eq!(c1, c2);
    }

    #[test]
    fn checksum_differs_for_different_content() {
        let b1 = BufferPlain::buffer_create(32);
        let mut b2 = BufferPlain::buffer_create(32);
        b2.data_mut()[0] = 0xFF;
        assert_ne!(b1.calc_checksum(), b2.calc_checksum());
    }
}

#[cfg(test)]
mod buffer_encrypted_tests {
    use super::*;

    #[test]
    fn create_has_correct_length() {
        let b = BufferEncrypted::buffer_create(512);
        assert_eq!(b.buffer_len(), 512);
    }

    #[test]
    fn from_plain_round_trips_via_to_plain() {
        let mut plain = BufferPlain::buffer_create(8);
        plain.data_mut().copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let enc = BufferEncrypted::from_plain(&plain);
        let back = enc.to_plain();
        assert_eq!(plain.data(), back.data());
    }

    #[test]
    fn copy_sz_pos_copies_correct_bytes() {
        let mut src = BufferEncrypted::buffer_create(16);
        src.data_mut()[0..4].copy_from_slice(&[0x11, 0x22, 0x33, 0x44]);
        let mut dst = BufferEncrypted::buffer_create(16);
        let copied = BufferEncrypted::copy_sz_pos(&src, 0, 4, &mut dst, 8);
        assert_eq!(copied, 4);
        assert_eq!(&dst.data()[8..12], &[0x11, 0x22, 0x33, 0x44]);
    }
}

#[cfg(test)]
mod record_tests {
    use super::*;

    #[test]
    fn mkaid_returns_64_hex_chars() {
        let c = test_config();
        let aid = mkaid(&c);
        assert_eq!(aid.len(), 64);
        assert!(aid.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn mkaid_unique_for_same_config() {
        let c1 = test_config();
        let c2 = test_config();
        // mkaid hashes 256 random bits, so successive calls produce different ids
        assert_ne!(mkaid(&c1), mkaid(&c2));
    }
}

#[cfg(test)]
mod assembly_plain_writable_tests {
    use super::*;

    #[test]
    fn create_returns_correct_buffer_size() {
        let c = test_config();
        let (info, buf) = assembly_plain_writable::create(&c);
        assert_eq!(buf.buffer_len(), CHUNKSIZE * info.nchunks.to_n());
    }

    #[test]
    fn create_returns_valid_nchunks() {
        let c = test_config();
        let (info, _) = assembly_plain_writable::create(&c);
        assert_eq!(info.nchunks.to_positive(), 16);
    }

    #[test]
    fn create_aid_is_64_hex_chars() {
        let c = test_config();
        let (info, _) = assembly_plain_writable::create(&c);
        assert_eq!(info.aid.len(), 64);
    }

    #[test]
    fn calc_checksum_returns_placeholder() {
        let b = BufferPlain::buffer_create(64);
        assert_eq!(assembly_plain_writable::calc_checksum(&b), "<>");
    }
}

#[cfg(test)]
mod assembly_encrypted_tests {
    use super::*;

    #[test]
    fn create_returns_correct_buffer_size() {
        let c = test_config();
        let (info, buf) = assembly_encrypted::create(&c);
        assert_eq!(buf.buffer_len(), CHUNKSIZE * info.nchunks.to_n());
    }

    #[test]
    fn create_apos_is_zero() {
        let c = test_config();
        let (info, _) = assembly_encrypted::create(&c);
        assert_eq!(info.apos, 0);
    }
}

#[cfg(test)]
mod assembly_plain_full_tests {
    use super::*;

    #[test]
    fn create_returns_correct_buffer_size() {
        let c = test_config();
        let (info, buf) = assembly_plain_full::create(&c);
        let sz = CHUNKSIZE * info.nchunks.to_n();
        assert_eq!(buf.buffer_len(), sz);
    }

    #[test]
    fn create_apos_equals_total_size() {
        let c = test_config();
        let (info, _) = assembly_plain_full::create(&c);
        assert_eq!(info.apos, CHUNKSIZE * info.nchunks.to_n());
    }
}

#[cfg(test)]
mod backup_restore_tests {
    use super::*;

    #[test]
    fn backup_writes_content_and_advances_apos() {
        let c = test_config();
        let (info, mut buf) = assembly_plain_writable::create(&c);
        let initial_apos = info.apos;

        let mut content = BufferPlain::buffer_create(128);
        for (i, b) in content.data_mut().iter_mut().enumerate() {
            *b = (i % 256) as u8;
        }

        let (info2, bi) = backup(&info, &mut buf, 0, &content);
        assert_eq!(bi.blocksize, 128);
        assert_eq!(bi.blockapos, initial_apos);
        assert_eq!(bi.filepos, 0);
        assert_eq!(info2.apos, initial_apos + 128);
    }

    #[test]
    fn backup_then_restore_round_trips() {
        let c = test_config();
        let (info, mut buf) = assembly_plain_writable::create(&c);

        let mut content = BufferPlain::buffer_create(256);
        for (i, b) in content.data_mut().iter_mut().enumerate() {
            *b = (i % 256) as u8;
        }

        let (_info2, bi) = backup(&info, &mut buf, 100, &content);
        let (_full_info, full_buf) = finish(info.clone(), buf);

        // buf was modified in-place, full_buf is the same buffer
        // Use the original buf reference through full_buf
        let restored = restore(&full_buf, &bi);
        assert!(restored.is_some());
        let restored_buf = restored.unwrap();
        assert_eq!(restored_buf.data(), content.data());
    }

    #[test]
    fn restore_fails_on_checksum_mismatch() {
        let c = test_config();
        let (info, mut buf) = assembly_plain_writable::create(&c);

        let content = BufferPlain::buffer_create(64);
        let (_info2, mut bi) = backup(&info, &mut buf, 0, &content);

        // corrupt the checksum
        bi.bchecksum = "badbadbadbadbadb".to_string();

        let result = restore(&buf, &bi);
        assert!(result.is_none());
    }

    #[test]
    fn multiple_backups_produce_non_overlapping_blocks() {
        let c = test_config();
        let (info, mut buf) = assembly_plain_writable::create(&c);

        let content1 = BufferPlain::buffer_create(100);
        let (info2, bi1) = backup(&info, &mut buf, 0, &content1);

        let content2 = BufferPlain::buffer_create(200);
        let (_info3, bi2) = backup(&info2, &mut buf, 100, &content2);

        assert_eq!(bi1.blockapos + bi1.blocksize, bi2.blockapos);
    }
}

#[cfg(test)]
mod encrypt_decrypt_tests {
    use super::*;

    #[test]
    fn encrypt_then_decrypt_round_trips() {
        let c = test_config();
        let (info, mut buf) = assembly_plain_writable::create(&c);

        // Write some known content
        let mut content = BufferPlain::buffer_create(512);
        for (i, b) in content.data_mut().iter_mut().enumerate() {
            *b = ((i * 7 + 3) % 256) as u8;
        }
        let (info2, _bi) = backup(&info, &mut buf, 0, &content);
        let (full_info, full_buf) = finish(info2, buf);

        let ki = KeyInformation {
            ivec: Key128::from_hex(&"a".repeat(32)).unwrap(),
            pkey: Key256::from_hex(&"b".repeat(64)).unwrap(),
            localnchunks: 16,
        };

        let enc_result = encrypt(&full_info, &full_buf, &ki);
        assert!(enc_result.is_some());
        let (enc_info, enc_buf) = enc_result.unwrap();

        // encrypted apos should equal full assembly size
        assert_eq!(enc_info.apos, assemblysize(&enc_info.nchunks));

        let dec_result = decrypt(&enc_info, &enc_buf, &ki);
        assert!(dec_result.is_some());
        let (dec_info, dec_buf) = dec_result.unwrap();

        assert_eq!(dec_info.apos, 0);
        assert_eq!(full_buf.data(), dec_buf.data());
    }

    #[test]
    fn decrypt_with_wrong_key_produces_different_content() {
        let c = test_config();
        let (info, buf) = assembly_plain_full::create(&c);

        let ki = KeyInformation {
            ivec: Key128::from_hex(&"1".repeat(32)).unwrap(),
            pkey: Key256::from_hex(&"2".repeat(64)).unwrap(),
            localnchunks: 16,
        };
        let ki_wrong = KeyInformation {
            ivec: Key128::from_hex(&"3".repeat(32)).unwrap(),
            pkey: Key256::from_hex(&"4".repeat(64)).unwrap(),
            localnchunks: 16,
        };

        // Write non-zero data so difference is detectable
        let mut plain = buf;
        for (i, b) in plain.data_mut().iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }

        let (_, enc_buf) = encrypt(&info, &plain, &ki).unwrap();
        let result = decrypt(&info, &enc_buf, &ki_wrong);

        assert!(
            result.is_none(),
            "decryption with wrong key should fail authentication"
        );
    }
    #[test]
    fn decrypt_under_different_aid_fails() {
        let c = test_config();
        let (info, buf) = assembly_plain_full::create(&c);
        let (other, _) = assembly_plain_full::create(&c);
        assert_ne!(info.aid, other.aid);

        let ki = KeyInformation {
            ivec: Key128::from_hex(&"1".repeat(32)).unwrap(),
            pkey: Key256::from_hex(&"2".repeat(64)).unwrap(),
            localnchunks: 16,
        };
        let (enc_info, enc_buf) = encrypt(&info, &buf, &ki).unwrap();
        assert!(decrypt(&enc_info, &enc_buf, &ki).is_some());

        // same key record, chunks presented as another assembly
        let swapped = AssemblyInformation {
            nchunks: enc_info.nchunks,
            aid: other.aid.clone(),
            apos: enc_info.apos,
        };
        assert!(
            decrypt(&swapped, &enc_buf, &ki).is_none(),
            "the aid is authenticated, assemblies cannot be swapped"
        );
    }
}

#[cfg(test)]
mod extract_recall_tests {
    use super::*;
    use std::fs;

    fn cleanup_dir(path: &str) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn extract_creates_chunk_files() {
        let chunk_dir = "/tmp/elykseer_test_extract_chunks";
        cleanup_dir(chunk_dir);

        let c = Configuration::new(
            16,
            chunk_dir.to_string(),
            "/tmp/elykseer_test_db".to_string(),
            "extract-test-id".to_string(),
            Tracer::null_tracer(),
        );

        let (info, enc_buf) = assembly_encrypted::create(&c);
        let nwritten = extract(&c, &info, &enc_buf);

        // All 16 chunks should be written, each CHUNKSIZE bytes
        assert_eq!(nwritten, CHUNKSIZE * 16);

        // Verify files exist
        for cid in 1..=16u64 {
            let path = chunk_identifier_path(&c, &info.aid, cid);
            assert!(
                std::path::Path::new(&path).is_file(),
                "chunk {} missing",
                cid
            );
            let data = fs::read(&path).unwrap();
            assert_eq!(data.len() as u64, CHUNKSIZE);
        }

        cleanup_dir(chunk_dir);
    }

    #[test]
    fn extract_then_recall_round_trips() {
        let chunk_dir = "/tmp/elykseer_test_recall_chunks";
        cleanup_dir(chunk_dir);

        let c = Configuration::new(
            16,
            chunk_dir.to_string(),
            "/tmp/elykseer_test_db".to_string(),
            "recall-test-id".to_string(),
            Tracer::null_tracer(),
        );

        // Create an encrypted assembly with known content
        let (info, mut enc_buf) = assembly_encrypted::create(&c);
        for (i, b) in enc_buf.data_mut().iter_mut().enumerate() {
            *b = ((i * 13 + 5) % 256) as u8;
        }

        let nwritten = extract(&c, &info, &enc_buf);
        assert_eq!(nwritten, enc_buf.buffer_len());

        // Now recall it
        let result = recall(&c, &info);
        assert!(result.is_some(), "recall should succeed after extract");
        let (_recalled_info, recalled_buf) = result.unwrap();

        assert_eq!(enc_buf.data(), recalled_buf.data());

        cleanup_dir(chunk_dir);
    }

    #[test]
    fn recall_fails_when_chunks_missing() {
        let chunk_dir = "/tmp/elykseer_test_recall_missing";
        cleanup_dir(chunk_dir);

        let c = Configuration::new(
            16,
            chunk_dir.to_string(),
            "/tmp/elykseer_test_db".to_string(),
            "missing-test-id".to_string(),
            Tracer::null_tracer(),
        );

        let (info, _) = assembly_encrypted::create(&c);
        let result = recall(&c, &info);
        assert!(result.is_none(), "recall should fail when no chunks exist");

        cleanup_dir(chunk_dir);
    }

    #[test]
    fn recall_rejects_invalid_aid() {
        let c = test_config();
        let info = AssemblyInformation {
            nchunks: Nchunks::new(16),
            aid: format!("../../{}", "a".repeat(58)),
            apos: 0,
        };
        assert!(recall(&c, &info).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn extract_writes_private_chunk_files_by_default() {
        use std::os::unix::fs::PermissionsExt;
        let chunk_dir = "/tmp/elykseer_test_extract_mode";
        cleanup_dir(chunk_dir);
        let c = Configuration::new(
            16,
            chunk_dir.to_string(),
            "/tmp/elykseer_test_db".to_string(),
            "mode-test-id".to_string(),
            Tracer::null_tracer(),
        );
        let (info, enc_buf) = assembly_encrypted::create(&c);
        extract(&c, &info, &enc_buf);
        let path = chunk_identifier_path(&c, &info.aid, 1);
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let dmode = fs::metadata(std::path::Path::new(&path).parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dmode & 0o077, 0);
        cleanup_dir(chunk_dir);
    }

    #[cfg(unix)]
    #[test]
    fn extract_honours_chunk_mode() {
        use std::os::unix::fs::PermissionsExt;
        let chunk_dir = "/tmp/elykseer_test_extract_mode644";
        cleanup_dir(chunk_dir);
        let c = Configuration::new(
            16,
            chunk_dir.to_string(),
            "/tmp/elykseer_test_db".to_string(),
            "mode-test-id".to_string(),
            Tracer::null_tracer(),
        )
        .with_chunk_mode(0o640);
        let (info, enc_buf) = assembly_encrypted::create(&c);
        extract(&c, &info, &enc_buf);
        let path = chunk_identifier_path(&c, &info.aid, 1);
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        // the umask may remove bits, it never adds any
        assert_eq!(mode & !0o640, 0);
        cleanup_dir(chunk_dir);
    }

    #[test]
    fn extract_does_not_overwrite_existing_chunks() {
        let chunk_dir = "/tmp/elykseer_test_no_overwrite";
        cleanup_dir(chunk_dir);

        let c = Configuration::new(
            16,
            chunk_dir.to_string(),
            "/tmp/elykseer_test_db".to_string(),
            "nooverwrite-test-id".to_string(),
            Tracer::null_tracer(),
        );

        let (info, enc_buf) = assembly_encrypted::create(&c);

        // First extract
        let n1 = extract(&c, &info, &enc_buf);
        assert_eq!(n1, CHUNKSIZE * 16);

        // Second extract should write 0 (files already exist)
        let n2 = extract(&c, &info, &enc_buf);
        assert_eq!(n2, 0);

        cleanup_dir(chunk_dir);
    }
}

#[cfg(test)]
mod chunk_identifier_tests {
    use super::*;

    // chunk id = SHA3-256(my_id ^ cid ^ aid); expected values computed
    // independently (Python hashlib.sha3_256) for my_id "test-id-001"
    const AID: &str = "abababababababababababababababababababababababababababababababab";

    #[test]
    fn chunk_identifier_known_answers() {
        let c = test_config();
        for (cid, expected) in [
            (
                1,
                "2d18aa1da49f169a65b8689f1080b2ef35096c7ae0e36212ef206ab55982900e",
            ),
            (
                3,
                "812126f17f90fdec6431a0101c5cd57e44fb208cd9beb4ff5d6559268469813d",
            ),
            (
                16,
                "6e542564b026606762e81fb80d909ff1537179129c8b8804fd7771d899764d32",
            ),
        ] {
            assert_eq!(chunk_identifier(&c, AID, cid), expected);
        }
    }

    #[test]
    fn chunk_identifier_depends_on_my_id() {
        let c = test_config();
        let mut other = test_config();
        other.my_id = "someone-else".to_string();
        assert_ne!(
            chunk_identifier(&c, AID, 1),
            chunk_identifier(&other, AID, 1)
        );
    }

    #[test]
    fn chunk_identifier_path_layout() {
        // <path_chunks>/<c63c64>/<c61c62>/<chunk id>.lxr
        let c = test_config();
        assert_eq!(
            chunk_identifier_path(&c, AID, 3),
            "/tmp/elykseer_test_chunks/3d/81/812126f17f90fdec6431a0101c5cd57e44fb208cd9beb4ff5d6559268469813d.lxr"
        );
    }
}

#[cfg(test)]
mod striping_tests {
    use super::*;

    fn pattern(len: usize) -> BufferPlain {
        let mut b = BufferPlain::buffer_create(len as u64);
        for (i, x) in b.data_mut().iter_mut().enumerate() {
            *x = (i % 251) as u8 + 1; // never zero
        }
        b
    }

    #[test]
    fn logical_byte_i_is_in_chunk_i_mod_n_at_offset_i_div_n() {
        for n in [16u64, 17, 29] {
            let mut asm = BufferPlain::buffer_create(n * CHUNKSIZE);
            let content = pattern(1000);
            let pos = 12345u64;
            assert_eq!(assembly_add_content(&content, 1000, pos, &mut asm), 1000);
            for (idx, &byte) in content.data().iter().enumerate() {
                let i = pos + idx as u64;
                let phys = (i % n) * CHUNKSIZE + i / n;
                assert_eq!(asm.data()[phys as usize], byte, "n={} i={}", n, i);
            }
            // nothing else was written
            let written = asm.data().iter().filter(|&&x| x != 0).count();
            assert_eq!(written, 1000);
        }
    }

    #[test]
    fn adjacent_bytes_are_one_chunk_apart() {
        let mut asm = BufferPlain::buffer_create(16 * CHUNKSIZE);
        let content = pattern(2);
        assembly_add_content(&content, 2, 0, &mut asm);
        assert_eq!(asm.data()[0], content.data()[0]);
        assert_eq!(asm.data()[CHUNKSIZE as usize], content.data()[1]);
    }

    #[test]
    fn get_content_is_the_inverse_of_add_content() {
        let mut asm = BufferPlain::buffer_create(17 * CHUNKSIZE);
        let content = pattern(40000);
        let pos = 17 * 999 + 5;
        assert_eq!(assembly_add_content(&content, 40000, pos, &mut asm), 40000);
        let mut out = BufferPlain::buffer_create(40000);
        assert_eq!(assembly_get_content(&asm, 40000, pos, &mut out), 40000);
        assert_eq!(out.data(), content.data());
    }

    #[test]
    fn rejects_ranges_that_do_not_fit() {
        let mut asm = BufferPlain::buffer_create(16 * CHUNKSIZE);
        let content = pattern(100);
        let end = 16 * CHUNKSIZE;
        assert_eq!(assembly_add_content(&content, 0, 0, &mut asm), 0);
        assert_eq!(assembly_add_content(&content, 101, 0, &mut asm), 0);
        assert_eq!(assembly_add_content(&content, 100, end - 99, &mut asm), 0);
        assert_eq!(
            assembly_add_content(&content, 100, end - 100, &mut asm),
            100
        );
        let mut out = BufferPlain::buffer_create(50);
        assert_eq!(assembly_get_content(&asm, 51, 0, &mut out), 0);
        assert_eq!(assembly_get_content(&asm, 50, end - 49, &mut out), 0);
        // not a multiple of the chunk size
        let mut odd = BufferPlain::buffer_create(1000);
        assert_eq!(assembly_add_content(&content, 100, 0, &mut odd), 0);
    }
}

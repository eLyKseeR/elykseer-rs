use elykseer_lib::common::cstdio::*;
use elykseer_lib::common::keys::{Key128, Key256};

// ===========================================================================
// EncryptionState
// ===========================================================================

#[cfg(test)]
mod encryption_state_tests {
    use super::*;

    #[test]
    fn buffer_plain_state_is_plain() {
        assert_eq!(BufferPlain::STATE, EncryptionState::Plain);
    }

    #[test]
    fn buffer_encrypted_state_is_encrypted() {
        assert_eq!(BufferEncrypted::STATE, EncryptionState::Encrypted);
    }
}

// ===========================================================================
// BufferPlain
// ===========================================================================

#[cfg(test)]
mod buffer_plain_tests {
    use super::*;

    #[test]
    fn buffer_create_has_correct_length() {
        let b = BufferPlain::buffer_create(1024);
        assert_eq!(b.buffer_len(), 1024);
    }

    #[test]
    fn buffer_create_zero_initialized() {
        let b = BufferPlain::buffer_create(64);
        assert!(b.data().iter().all(|&x| x == 0));
    }

    #[test]
    fn buffer_create_zero_size() {
        let b = BufferPlain::buffer_create(0);
        assert_eq!(b.buffer_len(), 0);
        assert!(b.is_empty());
    }

    #[test]
    fn copy_sz_pos_copies_correct_bytes() {
        let mut src = BufferPlain::buffer_create(16);
        src.data_mut()[0..4].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);

        let mut dst = BufferPlain::buffer_create(16);
        let copied = BufferPlain::copy_sz_pos(&src, 0, 4, &mut dst, 4);

        assert_eq!(copied, 4);
        assert_eq!(&dst.data()[4..8], &[0xAA, 0xBB, 0xCC, 0xDD]);
        assert_eq!(&dst.data()[0..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn copy_sz_pos_with_src_offset() {
        let mut src = BufferPlain::buffer_create(16);
        src.data_mut()[4..8].copy_from_slice(&[1, 2, 3, 4]);

        let mut dst = BufferPlain::buffer_create(16);
        let copied = BufferPlain::copy_sz_pos(&src, 4, 4, &mut dst, 0);

        assert_eq!(copied, 4);
        assert_eq!(&dst.data()[0..4], &[1, 2, 3, 4]);
    }

    #[test]
    fn copy_sz_pos_clips_to_src_bounds() {
        let src = BufferPlain::buffer_create(4);
        let mut dst = BufferPlain::buffer_create(16);
        let copied = BufferPlain::copy_sz_pos(&src, 2, 8, &mut dst, 0);
        assert_eq!(copied, 2);
    }

    #[test]
    fn copy_sz_pos_clips_to_dst_bounds() {
        let src = BufferPlain::buffer_create(16);
        let mut dst = BufferPlain::buffer_create(4);
        let copied = BufferPlain::copy_sz_pos(&src, 0, 8, &mut dst, 0);
        assert_eq!(copied, 4);
    }

    #[test]
    fn copy_sz_pos_clips_to_dst_offset_bounds() {
        let src = BufferPlain::buffer_create(16);
        let mut dst = BufferPlain::buffer_create(8);
        let copied = BufferPlain::copy_sz_pos(&src, 0, 8, &mut dst, 6);
        assert_eq!(copied, 2);
    }

    #[test]
    fn copy_sz_pos_zero_count() {
        let src = BufferPlain::buffer_create(8);
        let mut dst = BufferPlain::buffer_create(8);
        let copied = BufferPlain::copy_sz_pos(&src, 0, 0, &mut dst, 0);
        assert_eq!(copied, 0);
    }

    #[test]
    fn checksum_deterministic() {
        let b = BufferPlain::buffer_create(32);
        assert_eq!(b.calc_checksum(), b.calc_checksum());
    }

    #[test]
    fn checksum_differs_for_different_content() {
        let b1 = BufferPlain::buffer_create(32);
        let mut b2 = BufferPlain::buffer_create(32);
        b2.data_mut()[0] = 0xFF;
        assert_ne!(b1.calc_checksum(), b2.calc_checksum());
    }

    #[test]
    fn checksum_is_64_hex_chars() {
        let b = BufferPlain::buffer_create(64);
        let cs = b.calc_checksum();
        assert_eq!(cs.len(), 64);
        assert!(cs.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn checksum_is_sha3_256() {
        let b = BufferPlain::from_buffer(b"abc");
        assert_eq!(
            b.calc_checksum(),
            "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
        );
    }

    #[test]
    fn from_buffer_preserves_data() {
        let data = vec![10, 20, 30, 40];
        let b = BufferPlain::from_buffer(&data);
        assert_eq!(b.data(), &[10, 20, 30, 40]);
        assert_eq!(b.buffer_len(), 4);
    }

    #[test]
    fn to_buffer_returns_data() {
        let mut b = BufferPlain::buffer_create(4);
        b.data_mut().copy_from_slice(&[5, 6, 7, 8]);
        let raw = b.to_buffer();
        assert_eq!(raw, vec![5, 6, 7, 8]);
    }

    #[test]
    fn from_buffer_to_buffer_round_trip() {
        let original = vec![1, 2, 3, 4, 5];
        let b = BufferPlain::from_buffer(&original);
        assert_eq!(b.to_buffer(), original);
    }
}

// ===========================================================================
// BufferEncrypted
// ===========================================================================

#[cfg(test)]
mod buffer_encrypted_tests {
    use super::*;

    #[test]
    fn buffer_create_has_correct_length() {
        let b = BufferEncrypted::buffer_create(512);
        assert_eq!(b.buffer_len(), 512);
    }

    #[test]
    fn buffer_create_zero_initialized() {
        let b = BufferEncrypted::buffer_create(32);
        assert!(b.data().iter().all(|&x| x == 0));
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

    #[test]
    fn from_buffer_and_to_buffer_round_trip() {
        let raw = vec![0xAA, 0xBB, 0xCC];
        let b = BufferEncrypted::from_buffer(&raw);
        assert_eq!(b.to_buffer(), raw);
    }

    #[test]
    fn from_plain_and_to_plain_round_trip() {
        let mut plain = BufferPlain::buffer_create(8);
        plain.data_mut().copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let enc = BufferEncrypted::from_plain(&plain);
        let back = enc.to_plain();
        assert_eq!(plain.data(), back.data());
    }

    #[test]
    fn checksum_is_64_hex_chars() {
        let b = BufferEncrypted::buffer_create(64);
        let cs = b.calc_checksum();
        assert_eq!(cs.len(), 64);
        assert!(cs.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn checksum_matches_same_data_in_plain() {
        let data = vec![42u8; 100];
        let bp = BufferPlain::from_buffer(&data);
        let be = BufferEncrypted::from_buffer(&data);
        assert_eq!(bp.calc_checksum(), be.calc_checksum());
    }
}

// ===========================================================================
// encrypt / decrypt
// ===========================================================================

#[cfg(test)]
mod encrypt_decrypt_tests {
    use super::*;

    const AID: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn encrypt_then_decrypt_round_trips() {
        let mut plain = BufferPlain::buffer_create(256);
        for (i, b) in plain.data_mut().iter_mut().enumerate() {
            *b = ((i * 7 + 3) % 256) as u8;
        }

        let iv = Key128::from_hex(&"a".repeat(32)).unwrap();
        let pw = Key256::from_hex(&"b".repeat(64)).unwrap();

        let (enc_len, enc_buf) = encrypt(&plain, AID.as_bytes(), &iv, &pw);
        assert_eq!(enc_len, 256);

        let (dec_len, dec_buf) =
            decrypt(&enc_buf, AID.as_bytes(), &iv, &pw).expect("decryption should succeed");
        assert_eq!(dec_len, 256);
        // The first 240 bytes (256 - 16 tag) should match; the last 16 are tag area (zeroed).
        assert_eq!(&plain.data()[..240], &dec_buf.data()[..240]);
    }

    #[test]
    fn encrypt_produces_different_output() {
        let mut plain = BufferPlain::buffer_create(64);
        for (i, b) in plain.data_mut().iter_mut().enumerate() {
            *b = (i % 256) as u8;
        }

        let (_, enc) = encrypt(
            &plain,
            AID.as_bytes(),
            &Key128::random().unwrap(),
            &Key256::random().unwrap(),
        );
        assert_ne!(plain.data(), enc.data());
    }

    #[test]
    fn decrypt_with_wrong_key_fails_authentication() {
        let mut plain = BufferPlain::buffer_create(64);
        for (i, b) in plain.data_mut().iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }

        let (_, enc) = encrypt(
            &plain,
            AID.as_bytes(),
            &Key128::from_hex(&"1".repeat(32)).unwrap(),
            &Key256::from_hex(&"1".repeat(64)).unwrap(),
        );
        let result = decrypt(
            &enc,
            AID.as_bytes(),
            &Key128::from_hex(&"2".repeat(32)).unwrap(),
            &Key256::from_hex(&"2".repeat(64)).unwrap(),
        );
        assert!(
            result.is_none(),
            "decryption with wrong key should fail authentication"
        );
    }

    #[test]
    fn encrypt_preserves_length() {
        let plain = BufferPlain::buffer_create(1000);
        let (len, enc) = encrypt(
            &plain,
            AID.as_bytes(),
            &Key128::random().unwrap(),
            &Key256::random().unwrap(),
        );
        assert_eq!(len, 1000);
        assert_eq!(enc.buffer_len(), 1000);
    }

    fn kat_plain() -> BufferPlain {
        let mut plain = BufferPlain::buffer_create(64);
        for (i, b) in plain.data_mut()[..48].iter_mut().enumerate() {
            *b = ((i * 7 + 3) % 256) as u8;
        }
        plain
    }

    fn kat_keys() -> (Key128, Key256) {
        (
            Key128::from_hex(&"a".repeat(32)).unwrap(),
            Key256::from_hex(&"b".repeat(64)).unwrap(),
        )
    }

    #[test]
    fn decrypt_with_wrong_aid_fails_authentication() {
        let (iv, pw) = kat_keys();
        let (_, enc) = encrypt(&kat_plain(), AID.as_bytes(), &iv, &pw);
        let other_aid = "f".repeat(64);
        assert!(decrypt(&enc, other_aid.as_bytes(), &iv, &pw).is_none());
        assert!(decrypt(&enc, &[], &iv, &pw).is_none());
        assert!(decrypt(&enc, AID.as_bytes(), &iv, &pw).is_some());
    }

    #[test]
    fn decrypt_tampered_ciphertext_fails_authentication() {
        let (iv, pw) = kat_keys();
        let (_, enc) = encrypt(&kat_plain(), AID.as_bytes(), &iv, &pw);
        for pos in [0usize, 30, 47, 48, 63] {
            let mut raw = enc.to_buffer();
            raw[pos] ^= 0x01;
            let tampered = BufferEncrypted::from_buffer(&raw);
            assert!(
                decrypt(&tampered, AID.as_bytes(), &iv, &pw).is_none(),
                "flipped bit at {} must be detected",
                pos
            );
        }
    }

    #[test]
    fn decrypt_with_wrong_nonce_fails_authentication() {
        let (iv, pw) = kat_keys();
        let (_, enc) = encrypt(&kat_plain(), AID.as_bytes(), &iv, &pw);
        let iv2 = Key128::from_hex(&format!("{}{}", "a".repeat(23), "b".repeat(9))).unwrap();
        assert!(decrypt(&enc, AID.as_bytes(), &iv2, &pw).is_none());
    }

    /// Known-answer test: AES-256-GCM, key 0xbb.., nonce = first 12 bytes of
    /// ivec 0xaa.., AAD = aid as hex string. Expected value computed
    /// independently with Python `cryptography` (AESGCM).
    #[test]
    fn encrypt_known_answer() {
        let (iv, pw) = kat_keys();
        let (_, enc) = encrypt(&kat_plain(), AID.as_bytes(), &iv, &pw);
        let hex: String = enc.data().iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(
            hex,
            "2563f5913a8a7d68c21cabecc088dcdab7f4f6bb937a555de4b2c22e0660031c\
             fb478430e2c6d45ae7b7b11b6a6c8dc968d155f9232166ffbd73de1c986b6466"
        );
    }
}

// ===========================================================================
// ranbuf128
// ===========================================================================

#[cfg(test)]
mod ranbuf128_tests {
    use super::*;

    #[test]
    fn ranbuf128_returns_128_bytes() {
        let b = ranbuf128().unwrap();
        assert_eq!(b.buffer_len(), 128);
    }

    #[test]
    fn ranbuf128_is_not_all_zeros() {
        let b = ranbuf128().unwrap();
        assert!(b.data().iter().any(|&x| x != 0));
    }

    #[test]
    fn ranbuf128_successive_calls_differ() {
        let b1 = ranbuf128().unwrap();
        let b2 = ranbuf128().unwrap();
        // 128 random bytes from the OS CSPRNG
        assert_ne!(b1.data(), b2.data());
    }
}

// ===========================================================================
// File I/O: fopen, fclose, fread, fwrite, fflush, ftell, fseek
// ===========================================================================

#[cfg(test)]
mod file_io_tests {
    use super::*;
    use std::fs;

    fn temp_path(name: &str) -> String {
        format!("/tmp/elykseer_cstdio_test_{}", name)
    }

    fn cleanup(path: &str) {
        let _ = fs::remove_file(path);
    }

    #[test]
    fn fopen_write_then_read() {
        let path = temp_path("write_read");
        cleanup(&path);

        // Write
        {
            let mut fp = fopen(&path, WRITE_MODE).expect("fopen write");
            let data = b"hello cstdio";
            let n = fwrite(&mut fp, data.len() as u64, data).expect("fwrite");
            assert_eq!(n, data.len() as u64);
            fclose(fp).expect("fclose write");
        }

        // Read
        {
            let mut fp = fopen(&path, READ_MODE).expect("fopen read");
            let (cnt, buf) = fread(&mut fp, 1024).expect("fread");
            assert_eq!(cnt, 12);
            assert_eq!(&buf, b"hello cstdio");
            fclose(fp).expect("fclose read");
        }

        cleanup(&path);
    }

    #[test]
    fn fopen_read_nonexistent_returns_none() {
        let result = fopen("/tmp/elykseer_cstdio_test_nonexistent_file_xyz", READ_MODE);
        assert!(result.is_none());
    }

    #[test]
    fn fopen_write_new_fails_if_exists() {
        let path = temp_path("wx_exists");
        cleanup(&path);

        // Create the file first
        fs::write(&path, b"existing").unwrap();

        // write_new_mode should fail
        let result = fopen(&path, WRITE_NEW_MODE);
        assert!(result.is_none());

        cleanup(&path);
    }

    #[test]
    fn fopen_write_new_succeeds_if_not_exists() {
        let path = temp_path("wx_new");
        cleanup(&path);

        let fp = fopen(&path, WRITE_NEW_MODE);
        assert!(fp.is_some());
        fclose(fp.unwrap()).unwrap();

        assert!(std::path::Path::new(&path).exists());
        cleanup(&path);
    }

    #[test]
    fn fflush_returns_fptr() {
        let path = temp_path("fflush_test");
        cleanup(&path);

        let mut fp = fopen(&path, WRITE_MODE).unwrap();
        fwrite(&mut fp, 5, b"hello").unwrap();
        let fp2 = fflush(fp).expect("fflush should succeed");
        fclose(fp2).unwrap();

        cleanup(&path);
    }

    #[test]
    fn ftell_tracks_position() {
        let path = temp_path("ftell_test");
        cleanup(&path);

        let mut fp = fopen(&path, WRITE_MODE).unwrap();
        let pos0 = ftell(&mut fp).unwrap();
        assert_eq!(pos0, 0);

        fwrite(&mut fp, 10, &[0u8; 10]).unwrap();
        let pos1 = ftell(&mut fp).unwrap();
        assert_eq!(pos1, 10);

        fclose(fp).unwrap();
        cleanup(&path);
    }

    #[test]
    fn fseek_changes_read_position() {
        let path = temp_path("fseek_test");
        cleanup(&path);

        // Write some data
        {
            let mut fp = fopen(&path, WRITE_MODE).unwrap();
            fwrite(&mut fp, 10, b"0123456789").unwrap();
            fclose(fp).unwrap();
        }

        // Read from offset
        {
            let fp = fopen(&path, READ_MODE).unwrap();
            let fp = fseek(fp, 5).expect("fseek");
            let mut fp = fp;
            let (cnt, data) = fread(&mut fp, 5).unwrap();
            assert_eq!(cnt, 5);
            assert_eq!(&data, b"56789");
            fclose(fp).unwrap();
        }

        cleanup(&path);
    }

    #[test]
    fn fopen_append_mode_appends() {
        let path = temp_path("append_test");
        cleanup(&path);

        // Write initial data
        {
            let mut fp = fopen(&path, WRITE_MODE).unwrap();
            fwrite(&mut fp, 5, b"hello").unwrap();
            fclose(fp).unwrap();
        }

        // Append
        {
            let mut fp = fopen(&path, APPEND_MODE).unwrap();
            fwrite(&mut fp, 5, b"world").unwrap();
            fclose(fp).unwrap();
        }

        // Read all
        {
            let mut fp = fopen(&path, READ_MODE).unwrap();
            let (cnt, data) = fread(&mut fp, 1024).unwrap();
            assert_eq!(cnt, 10);
            assert_eq!(&data, b"helloworld");
            fclose(fp).unwrap();
        }

        cleanup(&path);
    }

    #[test]
    fn fopen_invalid_mode_returns_none() {
        let result = fopen("/tmp/elykseer_cstdio_test_invalid", "zz");
        assert!(result.is_none());
    }

    #[test]
    fn fwrite_partial_via_n() {
        let path = temp_path("fwrite_partial");
        cleanup(&path);

        let mut fp = fopen(&path, WRITE_MODE).unwrap();
        // data has 10 bytes, but we request writing only 5
        let n = fwrite(&mut fp, 5, b"0123456789").unwrap();
        assert_eq!(n, 5);
        fclose(fp).unwrap();

        let contents = fs::read(&path).unwrap();
        assert_eq!(&contents, b"01234");

        cleanup(&path);
    }

    #[test]
    fn fread_returns_fewer_bytes_at_eof() {
        let path = temp_path("fread_eof");
        cleanup(&path);

        fs::write(&path, b"abc").unwrap();

        let mut fp = fopen(&path, READ_MODE).unwrap();
        // request 1024 but file is only 3 bytes
        let (cnt, data) = fread(&mut fp, 1024).unwrap();
        assert_eq!(cnt, 3);
        assert_eq!(&data, b"abc");
        fclose(fp).unwrap();

        cleanup(&path);
    }
}

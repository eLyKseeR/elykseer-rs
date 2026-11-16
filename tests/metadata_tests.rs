use std::path::Path;
use std::sync::Arc;

use elykseer_lib::assembly::{BlockInformation, KeyInformation};
use elykseer_lib::common::configuration::Configuration;
use elykseer_lib::common::filesupport::FileInformation;
use elykseer_lib::common::keys::{Key128, Key256};
use elykseer_lib::common::metadata::*;
use elykseer_lib::common::tracer::Tracer;
use elykseer_lib::common::utilities::file_hash;
use elykseer_lib::processor::{Processor, is_safe_relative_path};

fn cleanup_dir(path: &str) {
    let _ = std::fs::remove_dir_all(path);
}

fn sample_block(blockid: u32) -> BlockInformation {
    BlockInformation {
        blockid,
        bchecksum: "c".repeat(64),
        blocksize: 32768,
        filepos: 32768 * (blockid as u64 - 1),
        blockaid: "a".repeat(64),
        blockapos: 16 + 32768 * (blockid as u64 - 1),
    }
}

fn sample_file(fname: &str) -> FileInformation {
    FileInformation {
        fname: fname.to_string(),
        fhash: file_hash(fname, "me"),
        fowner: "501".to_string(),
        fsize: 65536,
        fpermissions: 0o644,
        fmodified: "2026-10-12 12:00:00".to_string(),
        fchecksum: "d".repeat(64),
    }
}

fn sample_key() -> KeyInformation {
    KeyInformation {
        ivec: Key128::from_hex(&"1".repeat(32)).unwrap(),
        pkey: Key256::from_hex(&"2".repeat(64)).unwrap(),
        localnchunks: 16,
    }
}

// ===========================================================================
// JSON format (serde)
// ===========================================================================

#[cfg(test)]
mod json_tests {
    use super::*;

    #[test]
    fn relfile_round_trips_awkward_names() {
        for fname in [
            "/tmp/plain.txt",
            "/tmp/with \"quotes\".txt",
            "/tmp/back\\slash",
            "/tmp/new\nline\ttab\r",
            "/tmp/ctrl\u{1}\u{1f}\u{7f}",
            "/tmp/ünïcödé/日本語/😀",
            "/tmp/{brace}[bracket]:colon,comma",
        ] {
            let fi = sample_file(fname);
            let b1 = sample_block(1);
            let b2 = sample_block(2);
            let json = format_relfile_json(&fi, &[&b1, &b2]);
            let (fi2, blocks) = parse_relfile_json(&json).expect("valid JSON");
            assert_eq!(fi2, fi, "file information of {:?}", fname);
            assert_eq!(blocks, vec![b1.clone(), b2.clone()]);
        }
    }

    #[test]
    fn relfile_schema_uses_string_values() {
        let fi = sample_file("/tmp/x");
        let json = format_relfile_json(&fi, &[&sample_block(1)]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["fileinformation"]["fpermissions"], "644");
        assert_eq!(v["fileinformation"]["fsize"], "65536");
        assert_eq!(v["blocks"][0]["blockid"], "1");
        assert_eq!(v["blocks"][0]["blockapos"], "16");
        assert!(v["version"]["major"].is_string());
    }

    #[test]
    fn relfile_with_invalid_blockaid_is_rejected() {
        let fi = sample_file("/tmp/x");
        let mut b = sample_block(1);
        b.blockaid = "../../etc/passwd".to_string();
        let json = format_relfile_json(&fi, &[&b]);
        assert!(parse_relfile_json(&json).is_err());
    }

    #[test]
    fn relfile_with_bad_number_is_rejected() {
        let fi = sample_file("/tmp/x");
        let json = format_relfile_json(&fi, &[&sample_block(1)]).replace("\"32768\"", "\"x\"");
        assert!(parse_blocks_json(&json).is_err());
    }

    #[test]
    fn invalid_json_is_rejected() {
        assert!(parse_relfile_json("{ \"blocks\": [").is_err());
        assert!(parse_relfile_json("").is_err());
    }

    #[test]
    fn key_json_has_keyrel_schema() {
        let v: serde_json::Value = serde_json::from_str(&format_key_json(&sample_key())).unwrap();
        assert_eq!(v["keys"]["ivec"], "1".repeat(32));
        assert_eq!(v["keys"]["pkey"], "2".repeat(64));
        assert_eq!(v["keys"]["localnchunks"], "16");
    }
}

// ===========================================================================
// Reading with validation and permission checks
// ===========================================================================

#[cfg(test)]
mod read_tests {
    use super::*;

    fn write_key(db_base: &Path, aid: &str, mode: u32) {
        let dir = db_base.join("relkeys").join(hash_third_byte(aid));
        std::fs::create_dir_all(&dir).unwrap();
        let path = safe_json_path(&dir, aid);
        std::fs::write(&path, format_key_json(&sample_key())).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        #[cfg(not(unix))]
        let _ = mode;
    }

    #[test]
    fn read_key_json_accepts_private_file() {
        let db = "/tmp/elykseer_meta_test_key_ok";
        cleanup_dir(db);
        let aid = "ab".repeat(32);
        write_key(Path::new(db), &aid, 0o600);
        let ki = read_key_json(Path::new(db), &aid).expect("key readable");
        assert_eq!(ki.pkey, sample_key().pkey);
        assert_eq!(ki.ivec, sample_key().ivec);
        assert_eq!(ki.localnchunks, 16);
        cleanup_dir(db);
    }

    #[cfg(unix)]
    #[test]
    fn read_key_json_refuses_group_or_world_readable_file() {
        let db = "/tmp/elykseer_meta_test_key_open";
        cleanup_dir(db);
        let aid = "cd".repeat(32);
        write_key(Path::new(db), &aid, 0o644);
        let err = read_key_json(Path::new(db), &aid).unwrap_err();
        assert!(err.contains("too open"), "{}", err);
        write_key(Path::new(db), &aid, 0o640);
        assert!(read_key_json(Path::new(db), &aid).is_err());
        cleanup_dir(db);
    }

    #[test]
    fn read_key_json_rejects_invalid_aid() {
        assert!(read_key_json(Path::new("/tmp"), "../../etc/passwd").is_err());
        assert!(read_key_json(Path::new("/tmp"), &"A".repeat(64)).is_err());
    }

    #[test]
    fn read_blocks_json_rejects_invalid_fhash() {
        assert!(read_blocks_json(Path::new("/tmp"), "../x").is_err());
    }

    #[test]
    fn read_blocks_json_rejects_mismatching_fhash() {
        let db = "/tmp/elykseer_meta_test_fhash";
        cleanup_dir(db);
        let fi = sample_file("/tmp/a");
        let other = file_hash("/tmp/b", "me");
        let dir = Path::new(db).join("relfiles").join(hash_third_byte(&other));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            safe_json_path(&dir, &other),
            format_relfile_json(&fi, &[&sample_block(1)]),
        )
        .unwrap();
        assert!(read_blocks_json(Path::new(db), &other).is_err());
        cleanup_dir(db);
    }
}

// ===========================================================================
// Backup → metadata output → read back, file modes
// ===========================================================================

#[cfg(test)]
mod output_tests {
    use super::*;

    #[test]
    fn backup_writes_private_metadata_that_reads_back() {
        let base = "/tmp/elykseer_meta_test_out";
        cleanup_dir(base);
        let src = format!("{}/src/file \"one\".dat", base);
        std::fs::create_dir_all(format!("{}/src", base)).unwrap();
        let data: Vec<u8> = (0..100_000).map(|i| (i % 253) as u8).collect();
        std::fs::write(&src, &data).unwrap();

        let my_id = "meta-test";
        let c = Arc::new(Configuration::new(
            16,
            format!("{}/chunks", base),
            format!("{}/db", base),
            my_id.to_string(),
            Tracer::null_tracer(),
        ));
        let p = Processor::prepare(c);
        let p = p.file_backup(|_| None, |_| Vec::new(), Path::new(&src));
        let p = p.close();
        output_rel_keys_json(&p, false).unwrap();
        output_rel_files_json(&p, false).unwrap();

        let db_base = Path::new(base).join("db").join(my_id);
        let fhash = file_hash(&src, my_id);
        let blocks = read_blocks_json(&db_base, &fhash).expect("relfile readable");
        assert!(!blocks.is_empty());
        for bi in &blocks {
            read_key_json(&db_base, &bi.blockaid).expect("key readable");
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&db_base), 0o700);
            let aid = &blocks[0].blockaid;
            let keyp = safe_json_path(&db_base.join("relkeys").join(hash_third_byte(aid)), aid);
            assert_eq!(mode(&keyp), 0o600);
            assert_eq!(mode(keyp.parent().unwrap()), 0o700);
            let relp = safe_json_path(
                &db_base.join("relfiles").join(hash_third_byte(&fhash)),
                &fhash,
            );
            assert_eq!(mode(&relp), 0o600);
        }
        cleanup_dir(base);
    }
}

// ===========================================================================
// Restore target paths
// ===========================================================================

#[cfg(test)]
mod restore_path_tests {
    use super::*;

    #[test]
    fn safe_relative_paths() {
        assert!(is_safe_relative_path(Path::new("a/b/c.txt")));
        assert!(is_safe_relative_path(Path::new("./a")));
        assert!(is_safe_relative_path(Path::new("tmp/..x/y")));
    }

    #[test]
    fn unsafe_paths() {
        assert!(!is_safe_relative_path(Path::new("")));
        assert!(!is_safe_relative_path(Path::new("/etc/passwd")));
        assert!(!is_safe_relative_path(Path::new("../x")));
        assert!(!is_safe_relative_path(Path::new("a/../../x")));
        assert!(!is_safe_relative_path(Path::new("a/..")));
    }

    #[test]
    fn file_restore_refuses_parent_dir_components() {
        let base = "/tmp/elykseer_meta_test_restore";
        cleanup_dir(base);
        std::fs::create_dir_all(format!("{}/out", base)).unwrap();
        let c = Arc::new(Configuration::new(
            16,
            format!("{}/chunks", base),
            format!("{}/db", base),
            "restore-test".to_string(),
            Tracer::null_tracer(),
        ));
        let p = Processor::prepare(c);
        let (n, _p) = p.file_restore(
            Path::new(&format!("{}/out", base)),
            Path::new("../escaped.txt"),
            &[sample_block(1)],
        );
        assert_eq!(n, 0);
        assert!(!Path::new(&format!("{}/escaped.txt", base)).exists());
        cleanup_dir(base);
    }
}

// ===========================================================================
// Key material
// ===========================================================================

#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn debug_does_not_print_key_material() {
        let ki = sample_key();
        let dbg = format!("{:?}", ki);
        assert!(!dbg.contains(&"2".repeat(64)), "{}", dbg);
        assert!(!dbg.contains(&"1".repeat(32)), "{}", dbg);
        assert!(dbg.contains("redacted"));
    }

    #[test]
    fn keys_are_zeroize_on_drop() {
        fn assert_zod<T: zeroize::ZeroizeOnDrop>() {}
        assert_zod::<Key128>();
        assert_zod::<Key256>();
    }

    #[test]
    fn random_keys_differ() {
        assert_ne!(Key256::random().unwrap(), Key256::random().unwrap());
        assert_ne!(Key128::random().unwrap(), Key128::random().unwrap());
    }
}

// ===========================================================================
// Block numbering, stores, file checksum (elykseer-ml 0.10.3)
// ===========================================================================

#[cfg(test)]
mod spec_alignment_tests {
    use super::*;
    use elykseer_lib::common::filesupport::get_file_information;
    use elykseer_lib::common::store::Store;

    fn block_at(filepos: u64, aid: char) -> BlockInformation {
        BlockInformation {
            blockid: 1, // as set by assembly::backup
            bchecksum: "c".repeat(64),
            blocksize: 100,
            filepos,
            blockaid: aid.to_string().repeat(64),
            blockapos: filepos,
        }
    }

    fn config(db: &str) -> Arc<Configuration> {
        Arc::new(Configuration::new(
            16,
            "/tmp/elykseer_meta_test_chunks".to_string(),
            db.to_string(),
            "me".to_string(),
            Tracer::null_tracer(),
        ))
    }

    #[test]
    fn blocks_are_numbered_by_file_position() {
        let entries = vec![
            ("f2".to_string(), block_at(0, 'a')),
            ("f1".to_string(), block_at(200, 'a')),
            ("f1".to_string(), block_at(0, 'a')),
            ("f1".to_string(), block_at(100, 'a')),
        ];
        let files = consolidate_files(&entries);
        assert_eq!(files.len(), 2);
        let (fhash, blocks) = &files[0];
        assert_eq!(fhash, "f1");
        let numbered: Vec<(u32, u64)> = blocks.iter().map(|b| (b.blockid, b.filepos)).collect();
        assert_eq!(numbered, vec![(1, 0), (2, 100), (3, 200)]);
        assert_eq!(files[1].1.len(), 1);
    }

    #[test]
    fn a_position_backed_up_twice_keeps_the_newest_block() {
        let entries = vec![
            ("f".to_string(), block_at(0, 'a')),
            ("f".to_string(), block_at(100, 'a')),
            ("f".to_string(), block_at(0, 'b')),
        ];
        let blocks = &consolidate_files(&entries)[0].1;
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].blockaid, "b".repeat(64));
        assert_eq!(blocks[1].blockid, 2);
    }

    #[test]
    fn store_find_returns_the_entry_added_last() {
        let s = Store::init(config("/tmp/elykseer_meta_test_db_store"))
            .add("k", 1)
            .add("other", 2)
            .add("k", 3);
        assert_eq!(s.find("k"), Some(&3));
        assert_eq!(s.find_all("k"), vec![&1, &3]);
        assert_eq!(s.find("missing"), None);
    }

    #[test]
    fn fchecksum_is_the_sha3_256_of_the_content() {
        let path = "/tmp/elykseer_meta_test_fchecksum.txt";
        std::fs::write(path, b"abc").unwrap();
        let fi = get_file_information(&config("/tmp/elykseer_meta_test_db_fi"), path);
        assert_eq!(
            fi.fchecksum,
            "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
        );
        assert_eq!(fi.fsize, 3);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn relfile_hashes_are_listed() {
        let db = "/tmp/elykseer_meta_test_db_list";
        cleanup_dir(db);
        let base = Path::new(db).join("me");
        assert!(list_relfile_hashes(&base).is_empty());
        let mut expected = Vec::new();
        for name in ["/a/one", "/a/two"] {
            let fi = sample_file(name);
            let dir = base.join("relfiles").join(hash_third_byte(&fi.fhash));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                safe_json_path(&dir, &fi.fhash),
                format_relfile_json(&fi, &[&sample_block(1)]),
            )
            .unwrap();
            expected.push(fi.fhash.clone());
        }
        expected.sort();
        assert_eq!(list_relfile_hashes(&base), expected);
        let (fi, blocks) = read_relfile_json(&base, &expected[0]).unwrap();
        assert_eq!(fi.fsize, 65536);
        assert_eq!(blocks.len(), 1);
        cleanup_dir(db);
    }
}

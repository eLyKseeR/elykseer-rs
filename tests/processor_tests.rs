use std::path::{Path, PathBuf};
use std::sync::Arc;

use elykseer_lib::assembly::BlockInformation;
use elykseer_lib::assemblycache::WriteQueueEntity;
use elykseer_lib::common::configuration::Configuration;
use elykseer_lib::common::cstdio::BufferPlain;
use elykseer_lib::common::tracer::Tracer;
use elykseer_lib::processor::{BLOCK_SZ, CACHE_SZ, Processor};

fn test_config(suffix: &str) -> Arc<Configuration> {
    Arc::new(Configuration::new(
        16,
        format!("/tmp/elykseer_proc_test_{}", suffix),
        format!("/tmp/elykseer_proc_test_{}_db", suffix),
        format!("proc-test-{}", suffix),
        Tracer::null_tracer(),
    ))
}

fn test_config_debug(suffix: &str) -> Arc<Configuration> {
    Arc::new(Configuration::new(
        16,
        format!("/tmp/elykseer_proc_test_{}", suffix),
        format!("/tmp/elykseer_proc_test_{}_db", suffix),
        format!("proc-test-{}", suffix),
        Tracer::stdout_tracer_debug(),
    ))
}

fn cleanup_dir(path: &str) {
    let _ = std::fs::remove_dir_all(path);
}

// ===========================================================================
// prepare_processor
// ===========================================================================

#[cfg(test)]
mod prepare_tests {
    use super::*;

    #[test]
    fn prepare_creates_processor() {
        let c = test_config("prep1");
        let p = Processor::prepare(c);
        assert_eq!(p.cache.write_queue_len(), 0);
        assert_eq!(p.cache.read_queue_len(), 0);
    }

    #[test]
    fn cache_sz_is_3() {
        assert_eq!(CACHE_SZ, 3);
    }

    #[test]
    fn block_sz_is_32768() {
        assert_eq!(BLOCK_SZ, 32768);
    }
}

// ===========================================================================
// backup_block
// ===========================================================================

#[cfg(test)]
mod backup_block_tests {
    use super::*;

    fn make_wqe(fhash: &str, size: u64) -> WriteQueueEntity {
        let mut buf = BufferPlain::buffer_create(size);
        for (i, b) in buf.data_mut().iter_mut().enumerate() {
            *b = (i % 256) as u8;
        }
        WriteQueueEntity {
            qfhash: fhash.to_string(),
            qfpos: 0,
            qbuffer: buf,
        }
    }

    #[test]
    fn backup_block_enqueues() {
        let c = test_config("bb1");
        let p = Processor::prepare(c);
        let p = p.backup_block(make_wqe("f1", 128));
        assert_eq!(p.cache.write_queue_len(), 1);
    }

    #[test]
    fn backup_block_multiple() {
        let c = test_config("bb2");
        let mut p = Processor::prepare(c);
        for i in 0..5 {
            p = p.backup_block(make_wqe(&format!("f{}", i), 64));
        }
        assert_eq!(p.cache.write_queue_len(), 5);
    }
}

// ===========================================================================
// run_write_requests
// ===========================================================================

#[cfg(test)]
mod run_queue_tests {
    use super::*;

    #[test]
    fn run_write_requests_processes_queue() {
        let chunk_dir = "/tmp/elykseer_proc_test_rwq";
        cleanup_dir(chunk_dir);

        let c = test_config_debug("rwq");
        let p = Processor::prepare(c);
        let wqe = WriteQueueEntity {
            qfhash: "test_file".to_string(),
            qfpos: 0,
            qbuffer: BufferPlain::buffer_create(128),
        };
        let p = p.backup_block(wqe);
        assert_eq!(p.cache.write_queue_len(), 1);

        let p = p.run_write_requests();
        assert_eq!(p.cache.write_queue_len(), 0);

        // fblock store should have the entry
        let blocks = p.cache.acfbstore.find_all("test_file");
        assert_eq!(blocks.len(), 1);

        cleanup_dir(chunk_dir);
    }
}

// ===========================================================================
// close
// ===========================================================================

#[cfg(test)]
mod close_tests {
    use super::*;

    #[test]
    fn close_finalises() {
        let chunk_dir = "/tmp/elykseer_proc_test_close";
        cleanup_dir(chunk_dir);

        let c = test_config_debug("close");
        let p = Processor::prepare(c);
        let wqe = WriteQueueEntity {
            qfhash: "close_file".to_string(),
            qfpos: 0,
            qbuffer: BufferPlain::buffer_create(256),
        };
        let p = p.backup_block(wqe);
        let p = p.run_write_requests();
        let p = p.close();

        assert!(!p.cache.ackstore.entries.is_empty());

        cleanup_dir(chunk_dir);
    }
}

// ===========================================================================
// list_directory_entries
// ===========================================================================

#[cfg(test)]
mod directory_tests {
    use super::*;

    #[test]
    fn list_directory_entries_on_tmp() {
        // /tmp should exist and have some entries
        let (files, dirs) = Processor::list_directory_entries(Path::new("/tmp"));
        // Just check it doesn't crash; contents vary
        let _ = (files, dirs);
    }

    #[cfg(unix)]
    #[test]
    fn list_directory_entries_skips_symlinks() {
        let dir = "/tmp/elykseer_proc_test_symlinks";
        cleanup_dir(dir);
        std::fs::create_dir_all(format!("{}/sub", dir)).unwrap();
        std::fs::write(format!("{}/file", dir), b"x").unwrap();
        std::os::unix::fs::symlink(format!("{}/file", dir), format!("{}/link_to_file", dir))
            .unwrap();
        // a directory symlink pointing to its parent would loop forever
        std::os::unix::fs::symlink(dir, format!("{}/sub/loop", dir)).unwrap();
        std::os::unix::fs::symlink(format!("{}/sub", dir), format!("{}/link_to_dir", dir)).unwrap();

        let (files, dirs) = Processor::list_directory_entries(Path::new(dir));
        assert_eq!(files.0, vec![Path::new(dir).join("file")]);
        assert_eq!(dirs.0, vec![Path::new(dir).join("sub")]);
        let (files, dirs) = Processor::list_directory_entries(&Path::new(dir).join("sub"));
        assert!(files.0.is_empty() && dirs.0.is_empty());

        cleanup_dir(dir);
    }

    #[test]
    fn list_directory_entries_on_nonexistent() {
        let (files, dirs) = Processor::list_directory_entries(Path::new("/nonexistent_dir_xyz"));
        assert!(files.0.is_empty());
        assert!(dirs.0.is_empty());
    }
}

// ===========================================================================
// file_backup with real files
// ===========================================================================

#[cfg(test)]
mod file_backup_tests {
    use super::*;

    fn create_test_file(path: &str, size: usize) {
        let dir = Path::new(path).parent().unwrap();
        std::fs::create_dir_all(dir).unwrap();
        let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
        std::fs::write(path, &data).unwrap();
    }

    #[test]
    fn file_backup_small_file() {
        let chunk_dir = "/tmp/elykseer_proc_test_fb_small";
        let test_file = "/tmp/elykseer_proc_test_fb_small_src/testfile.dat";
        cleanup_dir(chunk_dir);
        cleanup_dir("/tmp/elykseer_proc_test_fb_small_src");

        create_test_file(test_file, 512);

        let c = test_config_debug("fb_small");
        let p = Processor::prepare(c);

        let p = p.file_backup(|_| None, |_| Vec::new(), Path::new(test_file));

        // Should have backed up the file — fblock store should have entries
        let blocks = p
            .cache
            .acfbstore
            .find_all(&elykseer_lib::common::utilities::file_hash(
                test_file,
                &p.config.my_id,
            ));
        assert!(
            !blocks.is_empty(),
            "should have block entries for backed up file"
        );

        cleanup_dir(chunk_dir);
        cleanup_dir("/tmp/elykseer_proc_test_fb_small_src");
    }

    #[test]
    fn file_backup_dedup_skips_identical() {
        let chunk_dir = "/tmp/elykseer_proc_test_fb_dedup";
        let test_file = "/tmp/elykseer_proc_test_fb_dedup_src/testfile.dat";
        cleanup_dir(chunk_dir);
        cleanup_dir("/tmp/elykseer_proc_test_fb_dedup_src");

        create_test_file(test_file, 256);

        let c = test_config_debug("fb_dedup");
        let p = Processor::prepare(c);

        // Compute the expected checksum so dedup kicks in
        let fi = elykseer_lib::common::filesupport::get_file_information(&p.config, test_file);

        let saved_checksum = fi.fchecksum.clone();
        let saved_fhash = fi.fhash.clone();

        let p = p.file_backup(
            move |fhash| {
                if fhash == saved_fhash {
                    Some(saved_checksum.clone())
                } else {
                    None
                }
            },
            |_| Vec::new(),
            Path::new(test_file),
        );

        // Dedup should have skipped the file — no blocks stored
        let blocks = p
            .cache
            .acfbstore
            .find_all(&elykseer_lib::common::utilities::file_hash(
                test_file,
                &p.config.my_id,
            ));
        assert!(blocks.is_empty(), "dedup should skip identical file");

        cleanup_dir(chunk_dir);
        cleanup_dir("/tmp/elykseer_proc_test_fb_dedup_src");
    }

    #[test]
    fn file_backup_large_file_multiple_blocks() {
        let chunk_dir = "/tmp/elykseer_proc_test_fb_large";
        let test_file = "/tmp/elykseer_proc_test_fb_large_src/largefile.dat";
        cleanup_dir(chunk_dir);
        cleanup_dir("/tmp/elykseer_proc_test_fb_large_src");

        // Create a file larger than BLOCK_SZ (32768)
        create_test_file(test_file, 80000);

        let c = test_config_debug("fb_large");
        let p = Processor::prepare(c);

        let p = p.file_backup(|_| None, |_| Vec::new(), Path::new(test_file));

        let fhash = elykseer_lib::common::utilities::file_hash(test_file, &p.config.my_id);
        let blocks = p.cache.acfbstore.find_all(&fhash);
        // 80000 / 32768 = 2 full blocks + 1 partial = 3 blocks
        assert!(
            blocks.len() >= 3,
            "large file should produce multiple blocks, got {}",
            blocks.len()
        );

        cleanup_dir(chunk_dir);
        cleanup_dir("/tmp/elykseer_proc_test_fb_large_src");
    }
}

// ===========================================================================
// Full round-trip: file_backup → close → file_restore
// ===========================================================================

#[cfg(test)]
mod round_trip_tests {
    use super::*;

    fn create_test_file(path: &str, data: &[u8]) {
        let dir = Path::new(path).parent().unwrap();
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(path, data).unwrap();
    }

    #[test]
    fn backup_and_restore_round_trip() {
        let chunk_dir = "/tmp/elykseer_proc_test_rt";
        let src_file = "/tmp/elykseer_proc_test_rt_src/original.dat";
        let restore_base = "/tmp/elykseer_proc_test_rt_restore";
        cleanup_dir(chunk_dir);
        cleanup_dir("/tmp/elykseer_proc_test_rt_src");
        cleanup_dir(restore_base);

        // Create test data
        let test_data: Vec<u8> = (0..1024).map(|i| ((i * 7 + 3) % 256) as u8).collect();
        create_test_file(src_file, &test_data);

        let c = test_config_debug("rt");
        let p = Processor::prepare(c);

        // Backup
        let p = p.file_backup(|_| None, |_| Vec::new(), Path::new(src_file));

        // Get block info before closing
        let fhash = elykseer_lib::common::utilities::file_hash(src_file, &p.config.my_id);
        let blocks: Vec<BlockInformation> = p
            .cache
            .acfbstore
            .find_all(&fhash)
            .into_iter()
            .cloned()
            .collect();
        assert!(!blocks.is_empty(), "should have blocks after backup");

        // Close to finalise
        let p = p.close();

        // Restore
        let (n, _p) = p.file_restore(Path::new(restore_base), Path::new("restored.dat"), &blocks);

        // Check restored file
        let restored_path = PathBuf::from(restore_base).join("restored.dat");
        assert!(restored_path.exists(), "restored file should exist");
        let restored_data = std::fs::read(&restored_path).unwrap();
        assert_eq!(
            restored_data, test_data,
            "restored data should match original"
        );
        assert!(n > 0, "should have written bytes");

        cleanup_dir(chunk_dir);
        cleanup_dir("/tmp/elykseer_proc_test_rt_src");
        cleanup_dir(restore_base);
    }
}

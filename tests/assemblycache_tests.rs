use std::sync::Arc;

use elykseer_lib::assembly::{BlockInformation, KeyInformation};
use elykseer_lib::assemblycache::{
    AssemblyCache, QSIZE, ReadQueueEntity, ReadQueueResult, WriteQueueEntity,
};
use elykseer_lib::common::configuration::Configuration;
use elykseer_lib::common::cstdio::BufferPlain;
use elykseer_lib::common::filesupport::FileInformation;
use elykseer_lib::common::keys::{Key128, Key256};
use elykseer_lib::common::tracer::Tracer;

fn test_config(path_suffix: &str) -> Arc<Configuration> {
    Arc::new(Configuration::new(
        16,
        format!("/tmp/elykseer_ac_test_{}", path_suffix),
        format!("/tmp/elykseer_ac_test_{}_db", path_suffix),
        format!("ac-test-{}", path_suffix),
        Tracer::null_tracer(),
    ))
}

fn test_config_debug(path_suffix: &str) -> Arc<Configuration> {
    Arc::new(Configuration::new(
        16,
        format!("/tmp/elykseer_ac_test_{}", path_suffix),
        format!("/tmp/elykseer_ac_test_{}_db", path_suffix),
        format!("ac-test-{}", path_suffix),
        Tracer::stdout_tracer_debug(),
    ))
}

fn cleanup_dir(path: &str) {
    let _ = std::fs::remove_dir_all(path);
}

// ===========================================================================
// prepare_assemblycache
// ===========================================================================

#[cfg(test)]
mod prepare_tests {
    use super::*;

    #[test]
    fn prepare_creates_empty_queues() {
        let c = test_config("prepare1");
        let ac = AssemblyCache::prepare(c, 3);
        assert_eq!(ac.write_queue_len(), 0);
        assert_eq!(ac.read_queue_len(), 0);
    }

    #[test]
    fn prepare_creates_empty_envs() {
        let c = test_config("prepare2");
        let ac = AssemblyCache::prepare(c, 3);
        assert_eq!(ac.envs_len(), 0);
    }

    #[test]
    fn prepare_creates_empty_stores() {
        let c = test_config("prepare3");
        let ac = AssemblyCache::prepare(c, 3);
        assert!(ac.ackstore.find("nonexistent").is_none());
        assert!(ac.acfbstore.find_all("nonexistent").is_empty());
        assert!(ac.acfistore.find("nonexistent").is_none());
    }
}

// ===========================================================================
// enqueue_write_request
// ===========================================================================

#[cfg(test)]
mod enqueue_write_tests {
    use super::*;

    fn make_write_req(fhash: &str) -> WriteQueueEntity {
        WriteQueueEntity {
            qfhash: fhash.to_string(),
            qfpos: 0,
            qbuffer: BufferPlain::buffer_create(64),
        }
    }

    #[test]
    fn enqueue_write_succeeds_on_empty() {
        let c = test_config("enqw1");
        let ac = AssemblyCache::prepare(c, 3);
        let (ok, ac) = ac.enqueue_write_request(make_write_req("f1"));
        assert!(ok);
        assert_eq!(ac.write_queue_len(), 1);
    }

    #[test]
    fn enqueue_write_succeeds_up_to_qsize() {
        let c = test_config("enqw2");
        let mut ac = AssemblyCache::prepare(c, 3);
        for i in 0..QSIZE {
            let (ok, ac2) = ac.enqueue_write_request(make_write_req(&format!("f{}", i)));
            assert!(ok, "failed at i={}", i);
            ac = ac2;
        }
        assert_eq!(ac.write_queue_len(), QSIZE);
    }

    #[test]
    fn enqueue_write_fails_when_full() {
        let c = test_config("enqw3");
        let mut ac = AssemblyCache::prepare(c, 3);
        for i in 0..QSIZE {
            let (_, ac2) = ac.enqueue_write_request(make_write_req(&format!("f{}", i)));
            ac = ac2;
        }
        let (ok, ac) = ac.enqueue_write_request(make_write_req("overflow"));
        assert!(!ok);
        assert_eq!(ac.write_queue_len(), QSIZE);
    }
}

// ===========================================================================
// enqueue_read_request
// ===========================================================================

#[cfg(test)]
mod enqueue_read_tests {
    use super::*;

    fn make_read_req(aid: &str) -> ReadQueueEntity {
        ReadQueueEntity {
            rqaid: aid.to_string(),
            rqapos: 0,
            rqrlen: 64,
            rqfpos: 0,
            rqchecksum: String::new(),
        }
    }

    #[test]
    fn enqueue_read_succeeds_on_empty() {
        let c = test_config("enqr1");
        let ac = AssemblyCache::prepare(c, 3);
        let (ok, ac) = ac.enqueue_read_request(make_read_req("aid1"));
        assert!(ok);
        assert_eq!(ac.read_queue_len(), 1);
    }

    #[test]
    fn enqueue_read_succeeds_up_to_qsize() {
        let c = test_config("enqr2");
        let mut ac = AssemblyCache::prepare(c, 3);
        for i in 0..QSIZE {
            let (ok, ac2) = ac.enqueue_read_request(make_read_req(&format!("aid{}", i)));
            assert!(ok, "failed at i={}", i);
            ac = ac2;
        }
        assert_eq!(ac.read_queue_len(), QSIZE);
    }

    #[test]
    fn enqueue_read_fails_when_full() {
        let c = test_config("enqr3");
        let mut ac = AssemblyCache::prepare(c, 3);
        for i in 0..QSIZE {
            let (_, ac2) = ac.enqueue_read_request(make_read_req(&format!("aid{}", i)));
            ac = ac2;
        }
        let (ok, ac) = ac.enqueue_read_request(make_read_req("overflow"));
        assert!(!ok);
        assert_eq!(ac.read_queue_len(), QSIZE);
    }
}

// ===========================================================================
// iterate_write_queue / iterate_read_queue on empty
// ===========================================================================

#[cfg(test)]
mod empty_queue_tests {
    use super::*;

    #[test]
    fn iterate_empty_read_queue_returns_nil() {
        let c = test_config("emptyrd");
        let ac = AssemblyCache::prepare(c, 3);
        let (results, _ac) = ac.iterate_read_queue();
        assert!(results.is_empty());
    }

    #[test]
    fn iterate_empty_write_queue_is_identity() {
        let c = test_config("emptywrt");
        let ac = AssemblyCache::prepare(c, 3);
        let ac2 = ac.iterate_write_queue();
        assert_eq!(ac2.write_queue_len(), 0);
    }
}

// ===========================================================================
// add_key / add_fileinformation / add_fileblockinformation
// ===========================================================================

#[cfg(test)]
mod store_tests {
    use super::*;

    #[test]
    fn add_key_then_find() {
        let c = test_config("addkey1");
        let ac = AssemblyCache::prepare(c, 3);
        let ki = KeyInformation {
            pkey: Key256::from_hex(&"0".repeat(64)).unwrap(),
            ivec: Key128::from_hex(&"0".repeat(32)).unwrap(),
            localnchunks: 16,
        };
        let ac = ac.add_key("aid123", ki.clone());
        let found = ac.ackstore.find("aid123");
        assert!(found.is_some());
        assert_eq!(found.unwrap().pkey, ki.pkey);
    }

    #[test]
    fn add_fileinformation_then_find() {
        let c = test_config("addfi1");
        let ac = AssemblyCache::prepare(c, 3);
        let fi = FileInformation {
            fname: "test.txt".to_string(),
            fhash: "abc123".to_string(),
            fowner: "user".to_string(),
            fsize: 1024,
            fpermissions: 0o644,
            fmodified: "20210831".to_string(),
            fchecksum: "chk".to_string(),
        };
        let ac = ac.add_fileinformation(&fi);
        let found = ac.acfistore.find("test.txt");
        assert!(found.is_some());
        assert_eq!(found.unwrap().fsize, 1024);
    }

    #[test]
    fn add_fileblockinformation_then_find_all() {
        let c = test_config("addfbi1");
        let ac = AssemblyCache::prepare(c, 3);
        let bi = BlockInformation {
            blockid: 1,
            bchecksum: "chk".to_string(),
            blocksize: 256,
            filepos: 0,
            blockaid: "aid1".to_string(),
            blockapos: 42,
        };
        let ac = ac.add_fileblockinformation("fhash1", bi.clone());
        let found = ac.acfbstore.find_all("fhash1");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].blockid, 1);
    }

    #[test]
    fn add_two_blocks_same_fhash_find_all() {
        let c = test_config("addfbi2");
        let ac = AssemblyCache::prepare(c, 3);
        let bi1 = BlockInformation {
            blockid: 1,
            bchecksum: "chk1".to_string(),
            blocksize: 256,
            filepos: 0,
            blockaid: "aid1".to_string(),
            blockapos: 42,
        };
        let bi2 = BlockInformation {
            blockid: 2,
            bchecksum: "chk2".to_string(),
            blocksize: 256,
            filepos: 256,
            blockaid: "aid1".to_string(),
            blockapos: 298,
        };
        let ac = ac.add_fileblockinformation("fhash1", bi1);
        let ac = ac.add_fileblockinformation("fhash1", bi2);
        let found = ac.acfbstore.find_all("fhash1");
        assert_eq!(found.len(), 2);
    }
}

// ===========================================================================
// write + flush round-trip
// ===========================================================================

#[cfg(test)]
mod write_flush_tests {
    use super::*;

    #[test]
    fn iterate_write_queue_processes_requests() {
        let chunk_dir = "/tmp/elykseer_ac_test_wrt_iter";
        cleanup_dir(chunk_dir);

        let c = test_config_debug("wrt_iter");
        let ac = AssemblyCache::prepare(c, 3);

        let mut content = BufferPlain::buffer_create(128);
        for (i, b) in content.data_mut().iter_mut().enumerate() {
            *b = (i % 256) as u8;
        }

        let req = WriteQueueEntity {
            qfhash: "file_a".to_string(),
            qfpos: 0,
            qbuffer: content,
        };

        let (ok, ac) = ac.enqueue_write_request(req);
        assert!(ok);
        assert_eq!(ac.write_queue_len(), 1);

        let ac = ac.iterate_write_queue();
        assert_eq!(ac.write_queue_len(), 0);

        // fblock store should have the block
        let blocks = ac.acfbstore.find_all("file_a");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].blocksize, 128);

        cleanup_dir(chunk_dir);
    }

    #[test]
    fn flush_extracts_and_stores_key() {
        let chunk_dir = "/tmp/elykseer_ac_test_flush";
        cleanup_dir(chunk_dir);

        let c = test_config_debug("flush");
        let ac = AssemblyCache::prepare(c, 3);

        // Write some content so apos > 0
        let content = BufferPlain::buffer_create(256);
        let req = WriteQueueEntity {
            qfhash: "flush_file".to_string(),
            qfpos: 0,
            qbuffer: content,
        };
        let (ok, ac) = ac.enqueue_write_request(req);
        assert!(ok);
        let ac = ac.iterate_write_queue();

        // Now flush
        let ac = ac.flush();

        // Key store should now have at least one entry
        // (the aid from the finalised assembly)
        assert!(!ac.ackstore.entries.is_empty());

        cleanup_dir(chunk_dir);
    }

    #[test]
    fn close_finalises_and_resets() {
        let chunk_dir = "/tmp/elykseer_ac_test_close";
        cleanup_dir(chunk_dir);

        let c = test_config_debug("close");
        let ac = AssemblyCache::prepare(c, 3);

        let content = BufferPlain::buffer_create(256);
        let req = WriteQueueEntity {
            qfhash: "close_file".to_string(),
            qfpos: 0,
            qbuffer: content,
        };
        let (ok, ac) = ac.enqueue_write_request(req);
        assert!(ok);
        let ac = ac.iterate_write_queue();

        let ac = ac.close();

        // Key store should have the finalised assembly key
        assert!(!ac.ackstore.entries.is_empty());
        // Env cache should be cleared
        assert_eq!(ac.envs_len(), 0);

        cleanup_dir(chunk_dir);
    }
}

// ===========================================================================
// Full round-trip: write → close → add_key → read
// ===========================================================================

#[cfg(test)]
mod round_trip_tests {
    use super::*;

    #[test]
    fn write_close_read_round_trip() {
        let chunk_dir = "/tmp/elykseer_ac_test_rt";
        cleanup_dir(chunk_dir);

        let c = test_config_debug("rt");
        let ac = AssemblyCache::prepare(Arc::clone(&c), 3);

        // Write content
        let mut content = BufferPlain::buffer_create(512);
        for (i, b) in content.data_mut().iter_mut().enumerate() {
            *b = ((i * 7 + 3) % 256) as u8;
        }
        let req = WriteQueueEntity {
            qfhash: "roundtrip.dat".to_string(),
            qfpos: 100,
            qbuffer: content.clone(),
        };
        let (ok, ac) = ac.enqueue_write_request(req);
        assert!(ok);
        let ac = ac.iterate_write_queue();

        // Get the block info
        let blocks = ac.acfbstore.find_all("roundtrip.dat");
        assert_eq!(blocks.len(), 1);
        let bi = blocks[0].clone();

        // Close to finalise
        let ac = ac.close();

        // Verify key was stored
        assert!(!ac.ackstore.entries.is_empty());
        let (_aid, _ki) = &ac.ackstore.entries[ac.ackstore.entries.len() - 1];

        // Now enqueue a read request using the block info
        let rreq = ReadQueueEntity {
            rqaid: bi.blockaid.clone(),
            rqapos: bi.blockapos,
            rqrlen: bi.blocksize,
            rqfpos: bi.filepos,
            rqchecksum: String::new(),
        };
        let (ok, ac) = ac.enqueue_read_request(rreq);
        assert!(ok);

        let (results, _ac) = ac.iterate_read_queue();
        assert_eq!(results.len(), 1, "should have one read result");
        assert_eq!(results[0].rresult.data(), content.data());

        cleanup_dir(chunk_dir);
    }
}

// ===========================================================================
// Block checksum on read (Coq: run_read_requests)
// ===========================================================================

#[cfg(test)]
mod read_checksum_tests {
    use super::*;

    /// Write one block, close the cache, and return it with the block's
    /// information and content.
    fn written(name: &str) -> (AssemblyCache, BlockInformation, BufferPlain, String) {
        let chunk_dir = format!("/tmp/elykseer_ac_test_{}", name);
        cleanup_dir(&chunk_dir);
        let c = test_config_debug(name);
        let ac = AssemblyCache::prepare(Arc::clone(&c), 3);
        let mut content = BufferPlain::buffer_create(700);
        for (i, b) in content.data_mut().iter_mut().enumerate() {
            *b = ((i * 13 + 1) % 256) as u8;
        }
        let (ok, ac) = ac.enqueue_write_request(WriteQueueEntity {
            qfhash: "f".to_string(),
            qfpos: 0,
            qbuffer: content.clone(),
        });
        assert!(ok);
        let ac = ac.iterate_write_queue();
        let bi = ac.acfbstore.find_all("f")[0].clone();
        (ac.close(), bi, content, chunk_dir)
    }

    fn read(ac: AssemblyCache, bi: &BlockInformation, checksum: &str) -> Vec<ReadQueueResult> {
        let (ok, ac) = ac.enqueue_read_request(ReadQueueEntity {
            rqaid: bi.blockaid.clone(),
            rqapos: bi.blockapos,
            rqrlen: bi.blocksize,
            rqfpos: bi.filepos,
            rqchecksum: checksum.to_string(),
        });
        assert!(ok);
        ac.iterate_read_queue().0
    }

    #[test]
    fn matching_checksum_is_accepted() {
        let (ac, bi, content, dir) = written("cks_ok");
        let res = read(ac, &bi, &bi.bchecksum);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].rresult.data(), content.data());
        cleanup_dir(&dir);
    }

    #[test]
    fn wrong_checksum_yields_no_result() {
        let (ac, bi, _, dir) = written("cks_bad");
        assert!(read(ac, &bi, &"0".repeat(64)).is_empty());
        cleanup_dir(&dir);
    }

    #[test]
    fn wrong_position_is_caught_by_the_checksum() {
        let (ac, mut bi, _, dir) = written("cks_pos");
        bi.blockapos += 1;
        let checksum = bi.bchecksum.clone();
        assert!(read(ac, &bi, &checksum).is_empty());
        cleanup_dir(&dir);
    }
}

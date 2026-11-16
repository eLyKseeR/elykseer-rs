use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::assembly::BlockInformation;
use crate::assemblycache::{AssemblyCache, ReadQueueEntity, ReadQueueResult, WriteQueueEntity};
use crate::common::configuration::Configuration;
use crate::common::cstdio::{self, BufferPlain};
use crate::common::filesupport::{FileInformation, get_file_information};
use crate::common::tracer::LogLevel;

// ---------------------------------------------------------------------------
// Distinguishable path lists
// ---------------------------------------------------------------------------

/// A list of file paths
#[derive(Debug, Clone, Default)]
pub struct FilePaths(pub Vec<PathBuf>);

/// A list of directory paths
#[derive(Debug, Clone, Default)]
pub struct DirPaths(pub Vec<PathBuf>);

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Coq: cache_sz : positive := 3.
pub const CACHE_SZ: usize = 3;

/// Coq: block_sz := 32768.
pub const BLOCK_SZ: u64 = 32768;

// ---------------------------------------------------------------------------
// Processor record
// ---------------------------------------------------------------------------

/// Coq: Record processor
pub struct Processor {
    pub config: Arc<Configuration>,
    pub cache: AssemblyCache,
}

impl Processor {
    /// Coq: prepare_processor
    pub fn prepare(c: Arc<Configuration>) -> Self {
        let cache = AssemblyCache::prepare(Arc::clone(&c), CACHE_SZ);
        Processor { config: c, cache }
    }

    // -----------------------------------------------------------------------
    // Methods section
    // -----------------------------------------------------------------------

    /// Coq: backup_block
    pub fn backup_block(self, wqe: WriteQueueEntity) -> Self {
        let (ok, cache) = self.cache.enqueue_write_request(wqe.clone());
        if ok {
            Processor {
                config: self.config,
                cache,
            }
        } else {
            // queue is full — iterate first, then try again
            let cache = cache.iterate_write_queue();
            let (ok2, cache) = cache.enqueue_write_request(wqe);
            if ok2 {
                Processor {
                    config: self.config,
                    cache,
                }
            } else {
                // bad bad — return unchanged
                Processor {
                    config: self.config,
                    cache,
                }
            }
        }
    }

    /// Coq: run_write_requests
    pub fn run_write_requests(self) -> Self {
        let cache = self.cache.iterate_write_queue();
        Processor {
            config: self.config,
            cache,
        }
    }

    /// Coq: close
    pub fn close(self) -> Self {
        let cache = self.cache.close();
        Processor {
            config: self.config,
            cache,
        }
    }

    // -----------------------------------------------------------------------
    // FileProcessor section
    // -----------------------------------------------------------------------

    /// Coq: prepare_blocks' — recursively build block info list for a file
    fn prepare_blocks_inner(
        fuel: usize,
        bid: u32,
        fpos: u64,
        fsz: u64,
        mut agg: Vec<BlockInformation>,
    ) -> Vec<BlockInformation> {
        if fuel == 0 || fsz == 0 {
            agg.reverse();
            return agg;
        }
        let bsz = if BLOCK_SZ < fsz { BLOCK_SZ } else { fsz };
        if bsz > 0 {
            agg.push(BlockInformation {
                blockid: bid,
                bchecksum: String::new(),
                blocksize: bsz,
                filepos: fpos,
                blockaid: String::new(),
                blockapos: 0,
            });
        }
        Self::prepare_blocks_inner(fuel - 1, bid + 1, fpos + bsz, fsz - bsz, agg)
    }

    /// Coq: zip_blocks — merge new block layout with existing blocks for deduplication
    fn zip_blocks(newbs: &[BlockInformation], curbs: &[BlockInformation]) -> Vec<BlockInformation> {
        let mut result = Vec::with_capacity(newbs.len());
        let mut cur_iter = curbs.iter();
        for h in newbs {
            if let Some(h_cur) = cur_iter.next() {
                if h.blocksize == h_cur.blocksize && h.filepos == h_cur.filepos {
                    result.push(BlockInformation {
                        blockid: h.blockid,
                        bchecksum: h_cur.bchecksum.clone(),
                        blocksize: h_cur.blocksize,
                        filepos: h_cur.filepos,
                        blockaid: h_cur.blockaid.clone(),
                        blockapos: h_cur.blockapos,
                    });
                } else {
                    result.push(h.clone());
                }
            } else {
                result.push(h.clone());
            }
        }
        result
    }

    /// Coq: prepare_blocks
    fn prepare_blocks(curbs: &[BlockInformation], fsz: u64) -> Vec<BlockInformation> {
        let n_blocks = if fsz == 0 {
            1
        } else {
            1 + (fsz + (BLOCK_SZ / 2) - 1) / BLOCK_SZ
        };
        let newbs = Self::prepare_blocks_inner(n_blocks as usize, 1, 0, fsz, Vec::new());
        if curbs.is_empty() {
            newbs
        } else {
            Self::zip_blocks(&newbs, curbs)
        }
    }

    /// Coq: read_block — a short read means the file shrank since its blocks
    /// were planned.
    fn read_block(fp: &mut cstdio::Fptr, fb: &BlockInformation) -> Option<BufferPlain> {
        let (nread, data) = cstdio::fread(fp, fb.blocksize)?;
        (nread == fb.blocksize).then(|| BufferPlain::from_buffer(&data))
    }

    /// Coq: rec_file_backup_inner — read each block from file and back it up.
    /// The flag is `false` if a block could not be read.
    fn rec_file_backup_inner(
        mut self,
        tgtfbs: &[BlockInformation],
        fhash: &str,
        fp: &mut cstdio::Fptr,
    ) -> (Self, bool) {
        for fb in tgtfbs {
            if !fp.seek_to(fb.filepos) {
                self.config.trace.log(
                    LogLevel::Warning,
                    &format!("failed to fseek in file: {}", fhash),
                );
                return (self, false);
            }
            let Some(b) = Self::read_block(fp, fb) else {
                self.config.trace.log(
                    LogLevel::Warning,
                    &format!(
                        "failed to read block (file changed during backup?) of file: {}",
                        fhash
                    ),
                );
                return (self, false);
            };

            // deduplication level 2: compare block checksums
            let found = !fb.bchecksum.is_empty() && b.calc_checksum() == fb.bchecksum;
            if found {
                self.config.trace.log(
                    LogLevel::Debug,
                    &format!("identical block found with id = {}", fb.blockid),
                );
                // keep old blockinformation
                self.cache = self.cache.add_fileblockinformation(fhash, fb.clone());
            } else {
                self = self.backup_block(WriteQueueEntity {
                    qfhash: fhash.to_string(),
                    qfpos: fb.filepos,
                    qbuffer: b,
                });
            }
        }
        (self, true)
    }

    /// Coq: open_file_backup — open file and back up its blocks.
    /// The flag is `false` if the file could not be opened or read; its
    /// file information is then not recorded, so no metadata is written
    /// for it.
    fn open_file_backup(self, fi: &FileInformation, tgtfbs: &[BlockInformation]) -> (Self, bool) {
        let Some(mut fptr) = cstdio::fopen(&fi.fname, cstdio::READ_MODE) else {
            self.config.trace.log(
                LogLevel::Warning,
                &format!("failed to open file: {}", fi.fname),
            );
            return (self, false);
        };

        let (mut proc, ok) = self.rec_file_backup_inner(tgtfbs, &fi.fhash, &mut fptr);
        let closed = cstdio::fclose(fptr).is_some();
        if ok {
            proc.config.trace.log(
                LogLevel::Info,
                &format!("block backup succeeded of file: {}", fi.fname),
            );
            if closed {
                proc.cache = proc.cache.add_fileinformation(fi);
            }
        } else {
            proc.config.trace.log(
                LogLevel::Warning,
                &format!("block backup failed of file: {}", fi.fname),
            );
        }
        (proc, ok)
    }

    /// Coq: file_backup — back up a file with deduplication
    pub fn file_backup<F, G>(self, find_fchecksum: F, find_fblocks: G, fp: &Path) -> Self
    where
        F: Fn(&str) -> Option<String>,
        G: Fn(&str) -> Vec<BlockInformation>,
    {
        let fname = fp.to_string_lossy().to_string();
        let fi = get_file_information(&self.config, &fname);

        // deduplication level 1: compare file checksums
        let found = match find_fchecksum(&fi.fhash) {
            None => false,
            Some(fchecksum) => fchecksum == fi.fchecksum,
        };

        if found {
            self.config.trace.log(
                LogLevel::Info,
                &format!("file content checksum identical to meta data: {}", fname),
            );
            return self;
        }

        let curbs = find_fblocks(&fi.fhash);
        let tgtbs = Self::prepare_blocks(&curbs, fi.fsize);

        // In Coq a failed file yields the processor from before the file. Here
        // the processor is consumed, so the blocks queued so far stay; without
        // file information they are reported when the metadata is written.
        let (proc, _ok) = self.open_file_backup(&fi, &tgtbs);
        proc.run_write_requests()
    }

    /// Coq: internal_restore_to — write read results to a file
    fn internal_restore_to(
        config: &Configuration,
        fptr: &mut cstdio::Fptr,
        lrres: &[ReadQueueResult],
    ) -> u64 {
        let mut total = 0u64;
        for rres in lrres {
            if !fptr.seek_to(rres.readrequest.rqfpos) {
                config.trace.log(
                    LogLevel::Warning,
                    &format!("failed to fseek to position: {}", rres.readrequest.rqfpos),
                );
                return 0;
            }
            match cstdio::fwrite(fptr, rres.readrequest.rqrlen, rres.rresult.data()) {
                None => return 0,
                Some(n) => total += n,
            }
        }
        total
    }

    /// Coq: restore_block_to — enqueue a read request for one block
    fn restore_block_to(
        config: &Configuration,
        fptr: &mut cstdio::Fptr,
        cache: AssemblyCache,
        block: &BlockInformation,
    ) -> (u64, AssemblyCache) {
        let rreq = ReadQueueEntity {
            rqaid: block.blockaid.clone(),
            rqapos: block.blockapos,
            rqrlen: block.blocksize,
            rqfpos: block.filepos,
            rqchecksum: block.bchecksum.clone(),
        };
        let (ok, cache) = cache.enqueue_read_request(rreq.clone());
        if ok {
            (0, cache)
        } else {
            // queue full — iterate first
            let (lrres, cache) = cache.iterate_read_queue();
            let n = Self::internal_restore_to(config, fptr, &lrres);
            let (_, cache) = cache.enqueue_read_request(rreq);
            if n > 0 { (n, cache) } else { (0, cache) }
        }
    }

    /// Coq: restore_file_to — restore all blocks of a file
    fn restore_file_to(
        config: &Configuration,
        fptr: &mut cstdio::Fptr,
        cache: AssemblyCache,
        blocks: &[BlockInformation],
    ) -> (u64, AssemblyCache) {
        let mut total = 0u64;
        let mut cache = cache;
        for block in blocks {
            let (n, c) = Self::restore_block_to(config, fptr, cache, block);
            total += n;
            cache = c;
        }
        // flush remaining read queue
        let (lrres, cache) = cache.iterate_read_queue();
        let n = Self::internal_restore_to(config, fptr, &lrres);
        (n + total, cache)
    }

    /// Coq: file_restore — restore a file from its blocks
    ///
    /// `fp` must be a relative path without `..` components, so the restored
    /// file stays below `basep`.
    pub fn file_restore(self, basep: &Path, fp: &Path, blocks: &[BlockInformation]) -> (u64, Self) {
        let tracer = &self.config.trace;
        if !is_safe_relative_path(fp) {
            tracer.log(
                LogLevel::Warning,
                &format!("refusing to restore to unsafe path: {}", fp.display()),
            );
            return (0, self);
        }
        let targetp = basep.join(fp);

        if targetp.exists() {
            tracer.log(
                LogLevel::Warning,
                &format!("file already exists: {}", targetp.display()),
            );
            return (0, self);
        }

        tracer.log(
            LogLevel::Info,
            &format!(
                "restoring file {} from {} blocks",
                fp.display(),
                blocks.len()
            ),
        );

        // ensure parent directory exists
        if let Some(dirp) = targetp.parent()
            && !dirp.is_dir()
            && std::fs::create_dir_all(dirp).is_err()
        {
            return (0, self);
        }

        let mut fptr = match cstdio::fopen(&targetp.to_string_lossy(), cstdio::WRITE_NEW_MODE) {
            Some(fp) => fp,
            None => {
                tracer.log(
                    LogLevel::Warning,
                    &format!("failed to open file: {}", targetp.display()),
                );
                return (0, self);
            }
        };

        let (n, cache) = Self::restore_file_to(&self.config, &mut fptr, self.cache, blocks);
        let _ = cstdio::fclose(fptr);

        (
            n,
            Processor {
                config: self.config,
                cache,
            },
        )
    }

    // -----------------------------------------------------------------------
    // DirectoryProcessor section
    // -----------------------------------------------------------------------

    /// Coq: list_directory_entries
    /// Symlinks are skipped: following them can loop forever.
    pub fn list_directory_entries(fp: &Path) -> (FilePaths, DirPaths) {
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        if let Ok(entries) = std::fs::read_dir(fp) {
            for entry in entries.flatten() {
                // file_type() does not follow symlinks
                match entry.file_type() {
                    Ok(t) if t.is_dir() => dirs.push(entry.path()),
                    Ok(t) if t.is_file() => files.push(entry.path()),
                    _ => {}
                }
            }
        }
        (FilePaths(files), DirPaths(dirs))
    }
}

/// True if `p` is non-empty, relative and contains only normal components
/// (and `.`), i.e. joining it onto a directory cannot escape that directory.
pub fn is_safe_relative_path(p: &Path) -> bool {
    use std::path::Component;
    p.components().next().is_some()
        && p.components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::tracer::Tracer;

    /// A file that is shorter than its planned blocks (it shrank after the
    /// blocks were planned) fails; no file information is recorded for it.
    #[test]
    fn short_read_fails_the_file() {
        let dir = std::env::temp_dir().join("elykseer_proc_unit_short_read");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shrunk.bin");
        std::fs::write(&path, vec![7u8; 1000]).unwrap();

        let c = Arc::new(Configuration::new(
            16,
            dir.join("chunks").to_string_lossy().to_string(),
            dir.join("db").to_string_lossy().to_string(),
            "unit".to_string(),
            Tracer::null_tracer(),
        ));
        let fi = get_file_information(&c, &path.to_string_lossy());
        let planned = Processor::prepare_blocks(&[], 2000);
        assert_eq!(planned.len(), 1);

        let (proc, ok) = Processor::prepare(Arc::clone(&c)).open_file_backup(&fi, &planned);
        assert!(!ok, "a short read must fail the file");
        assert!(proc.cache.acfistore.find(&fi.fname).is_none());
        assert_eq!(proc.cache.write_queue_len(), 0);

        // the same file with matching blocks succeeds
        let planned = Processor::prepare_blocks(&[], fi.fsize);
        let (proc, ok) = proc.open_file_backup(&fi, &planned);
        assert!(ok, "backup of a readable file must succeed");
        assert!(proc.cache.acfistore.find(&fi.fname).is_some());
        assert_eq!(proc.cache.write_queue_len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

use std::sync::Arc;

use crate::assembly::{AidT, BlockInformation, KeyInformation, assembly_get_content};
use crate::common::configuration::Configuration;
use crate::common::cstdio::BufferPlain;
use crate::common::filesupport::FileInformation;
use crate::common::store::{FBlockListStore, FileinformationStore, KeyListStore};
use crate::environment::{EnvironmentReadable, EnvironmentWritable};

// ---------------------------------------------------------------------------
// Queue entities and results
// ---------------------------------------------------------------------------

/// Coq: Record readqueueentity
#[derive(Debug, Clone)]
pub struct ReadQueueEntity {
    pub rqaid: AidT,
    pub rqapos: u64,
    pub rqrlen: u64,
    pub rqfpos: u64,
    /// checksum of the block in the metadata; "" = unknown
    pub rqchecksum: String,
}

/// Coq: Record readqueueresult
#[derive(Debug, Clone)]
pub struct ReadQueueResult {
    pub readrequest: ReadQueueEntity,
    pub rresult: BufferPlain,
}

/// Coq: Record writequeueentity
#[derive(Debug, Clone)]
pub struct WriteQueueEntity {
    pub qfhash: String,
    pub qfpos: u64,
    /// implicit write data size = buffer size
    pub qbuffer: BufferPlain,
}

// ---------------------------------------------------------------------------
// Queues
// ---------------------------------------------------------------------------

/// Coq: Definition qsize : positive := 12.
pub const QSIZE: usize = 12;

/// Coq: Record readqueue
#[derive(Clone)]
struct ReadQueue {
    rqueue: Vec<ReadQueueEntity>,
}

/// Coq: Record writequeue
#[derive(Clone)]
struct WriteQueue {
    wqueue: Vec<WriteQueueEntity>,
}

// ---------------------------------------------------------------------------
// AssemblyCache
// ---------------------------------------------------------------------------

/// Coq: Record assemblycache
pub struct AssemblyCache {
    acenvs: Vec<EnvironmentReadable>,
    acsize: usize,
    acwriteenv: EnvironmentWritable,
    acconfig: Arc<Configuration>,
    acwriteq: WriteQueue,
    acreadq: ReadQueue,
    pub acfbstore: FBlockListStore,
    pub ackstore: KeyListStore,
    pub acfistore: FileinformationStore,
}

impl AssemblyCache {
    /// Coq: prepare_assemblycache
    pub fn prepare(c: Arc<Configuration>, size: usize) -> Self {
        let write_env = EnvironmentWritable::initial_environment(Arc::clone(&c));
        AssemblyCache {
            acenvs: Vec::new(),
            acsize: size,
            acwriteenv: write_env,
            acwriteq: WriteQueue { wqueue: Vec::new() },
            acreadq: ReadQueue { rqueue: Vec::new() },
            acfbstore: FBlockListStore::init(Arc::clone(&c)),
            ackstore: KeyListStore::init(Arc::clone(&c)),
            acfistore: FileinformationStore::init(Arc::clone(&c)),
            acconfig: c,
        }
    }

    /// Coq: enqueue_write_request
    pub fn enqueue_write_request(mut self, req: WriteQueueEntity) -> (bool, Self) {
        let ln = self.acwriteq.wqueue.len();
        if ln >= QSIZE {
            (false, self)
        } else {
            self.acwriteq.wqueue.push(req);
            (true, self)
        }
    }

    /// Coq: enqueue_read_request
    pub fn enqueue_read_request(mut self, req: ReadQueueEntity) -> (bool, Self) {
        let ln = self.acreadq.rqueue.len();
        if ln >= QSIZE {
            (false, self)
        } else {
            self.acreadq.rqueue.push(req);
            (true, self)
        }
    }

    /// Coq: try_restore_assembly
    fn try_restore_assembly(&self, sel_aid: &str) -> Option<EnvironmentReadable> {
        let ki = self.ackstore.find(sel_aid)?;
        EnvironmentReadable::restore_assembly(&self.acconfig, sel_aid, ki)
    }

    /// Coq: add_fileinformation
    pub fn add_fileinformation(mut self, fi: &FileInformation) -> Self {
        self.acfistore = self.acfistore.add(&fi.fname, fi.clone());
        self
    }

    /// Coq: add_fileblockinformation
    pub fn add_fileblockinformation(mut self, fhash: &str, bi: BlockInformation) -> Self {
        self.acfbstore = self.acfbstore.add(fhash, bi);
        self
    }

    /// Add a key to the key store (`KeyListStore.add`); used by the restore
    /// tool to load the keys of the assemblies it needs.
    pub fn add_key(mut self, aid: &str, ki: KeyInformation) -> Self {
        self.ackstore = self.ackstore.add(aid, ki);
        self
    }

    /// Coq: ensure_assembly — ensure a readable environment with the given aid
    /// is available and in the head position of acenvs.
    /// Returns the environment (cloned) and mutates acenvs in place.
    fn ensure_assembly(&mut self, sel_aid: &str) -> Option<EnvironmentReadable> {
        if self.acenvs.is_empty() {
            let env = self.try_restore_assembly(sel_aid)?;
            self.acenvs = vec![env.clone()];
            Some(env)
        } else if self.acenvs.len() == 1 {
            if self.acenvs[0].cur_assembly.aid == sel_aid {
                Some(self.acenvs[0].clone())
            } else {
                let env = self.try_restore_assembly(sel_aid)?;
                if self.acsize == 1 {
                    self.acenvs = vec![env.clone()];
                } else {
                    let e1 = self.acenvs[0].clone();
                    self.acenvs = vec![env.clone(), e1];
                }
                Some(env)
            }
        } else {
            // multiple envs
            if self.acenvs[0].cur_assembly.aid == sel_aid {
                return Some(self.acenvs[0].clone());
            }
            // search in rest
            let found_idx = self.acenvs[1..]
                .iter()
                .position(|e| e.cur_assembly.aid == sel_aid);
            if let Some(idx) = found_idx {
                let actual_idx = idx + 1;
                let efound = self.acenvs[actual_idx].clone();
                let mut new_envs = vec![efound.clone()];
                for (i, e) in self.acenvs.iter().enumerate() {
                    if i != actual_idx {
                        new_envs.push(e.clone());
                    }
                }
                self.acenvs = new_envs;
                Some(efound)
            } else {
                // not found — try to restore
                let env = self.try_restore_assembly(sel_aid)?;
                if self.acsize <= self.acenvs.len() {
                    self.acenvs.pop();
                }
                let mut new_envs = vec![env.clone()];
                new_envs.append(&mut self.acenvs);
                self.acenvs = new_envs;
                Some(env)
            }
        }
    }

    /// Coq: run_read_requests
    fn run_read_requests(
        &mut self,
        reqs: Vec<ReadQueueEntity>,
        mut res: Vec<ReadQueueResult>,
    ) -> Vec<ReadQueueResult> {
        for req in reqs {
            let rqrlen = req.rqrlen;
            let rqapos = req.rqapos;
            match self.ensure_assembly(&req.rqaid) {
                None => return res,
                Some(env) => {
                    let mut buf = BufferPlain::buffer_create(rqrlen);
                    let n = assembly_get_content(&env.cur_buffer, rqrlen, rqapos, &mut buf);
                    // the checksum catches a metadata entry that points to the wrong place
                    let ok = n > 0
                        && (req.rqchecksum.is_empty() || buf.calc_checksum() == req.rqchecksum);
                    if ok {
                        res.push(ReadQueueResult {
                            readrequest: req,
                            rresult: buf,
                        });
                    }
                }
            }
        }
        res
    }

    /// Coq: run_write_requests
    fn run_write_requests(self, reqs: Vec<WriteQueueEntity>) -> Self {
        let mut ac = self;
        for req in reqs {
            let fhash = req.qfhash.clone();
            let fpos = req.qfpos;
            let (env, (bi, kis)) = ac.acwriteenv.backup(&fhash, fpos, &req.qbuffer);
            if let Some((aid, ki)) = kis {
                ac.ackstore = ac.ackstore.add(&aid, ki);
            }
            ac.acfbstore = ac.acfbstore.add(&fhash, bi);
            ac.acwriteenv = env;
            ac.acwriteq.wqueue.clear();
        }
        ac
    }

    /// Coq: iterate_read_queue
    pub fn iterate_read_queue(mut self) -> (Vec<ReadQueueResult>, Self) {
        let rq = std::mem::take(&mut self.acreadq.rqueue);
        if rq.is_empty() {
            (Vec::new(), self)
        } else {
            let res = self.run_read_requests(rq, Vec::new());
            (res, self)
        }
    }

    /// Coq: iterate_write_queue
    pub fn iterate_write_queue(mut self) -> Self {
        let wq = std::mem::take(&mut self.acwriteq.wqueue);
        if wq.is_empty() {
            self
        } else {
            self.run_write_requests(wq)
        }
    }

    /// Coq: flush — finalise and recreate writable environment, extract chunks
    pub fn flush(mut self) -> Self {
        match self.acwriteenv.finalise_and_recreate_assembly() {
            None => self,
            Some((env, (aid, ki))) => {
                self.ackstore = self.ackstore.add(&aid, ki);
                self.acenvs.clear();
                self.acwriteenv = env;
                self
            }
        }
    }

    /// Coq: close — terminate cache: finalise writable environment
    pub fn close(mut self) -> Self {
        match self.acwriteenv.finalise_assembly() {
            None => self,
            Some((aid, ki)) => {
                self.ackstore = self.ackstore.add(&aid, ki);
                self.acenvs.clear();
                self.acwriteenv =
                    EnvironmentWritable::initial_environment(Arc::clone(&self.acconfig));
                self
            }
        }
    }

    // --- Accessors for testing ---

    pub fn write_queue_len(&self) -> usize {
        self.acwriteq.wqueue.len()
    }

    pub fn read_queue_len(&self) -> usize {
        self.acreadq.rqueue.len()
    }

    pub fn envs_len(&self) -> usize {
        self.acenvs.len()
    }

    pub fn config(&self) -> &Arc<Configuration> {
        &self.acconfig
    }
}

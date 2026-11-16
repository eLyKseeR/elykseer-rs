use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::assembly::{BlockInformation, KeyInformation};
use crate::common::filesupport::FileInformation;
use crate::common::fsutils::{
    PRIVATE_DIR_MODE, PRIVATE_FILE_MODE, check_private_file, create_dir_all_mode, write_file_mode,
};
use crate::common::keys::{Key128, Key256};
use crate::common::utilities::is_hex_id;
use crate::processor::Processor;

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Extract the hex pair at byte position 2 (chars 4–5) of a hex identifier.
pub fn hash_third_byte(h: &str) -> &str {
    if h.len() >= 6 { &h[4..6] } else { "00" }
}

/// Build a JSON path that avoids exceeding the 255-byte filename limit.
pub fn safe_json_path(base: &Path, name: &str) -> PathBuf {
    let filename = format!("{}.json", name);
    if filename.len() <= 255 {
        base.join(filename)
    } else {
        let split = name.len().saturating_sub(8);
        let prefix = &name[..split];
        let suffix = &name[split..];
        base.join(prefix).join(format!("{}.json", suffix))
    }
}

/// Create `dir` (and parents) with mode 0700 and make sure `dir` itself is
/// 0700 even if it existed before.
pub fn ensure_private_dir(dir: &Path) -> Result<(), String> {
    create_dir_all_mode(dir, PRIVATE_DIR_MODE)
        .map_err(|e| format!("cannot create directory {}: {}", dir.display(), e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(PRIVATE_DIR_MODE))
            .map_err(|e| format!("cannot set permissions of {}: {}", dir.display(), e))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// JSON documents (relkeys, relfiles); all values are JSON strings
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct VersionJson {
    major: String,
    minor: String,
    build: String,
}

impl VersionJson {
    fn current() -> Self {
        VersionJson {
            major: env!("CARGO_PKG_VERSION_MAJOR").to_string(),
            minor: env!("CARGO_PKG_VERSION_MINOR").to_string(),
            build: env!("CARGO_PKG_VERSION_PATCH").to_string(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct KeysJson {
    ivec: String,
    pkey: String,
    localnchunks: String,
}

impl Drop for KeysJson {
    fn drop(&mut self) {
        self.ivec.zeroize();
        self.pkey.zeroize();
    }
}

#[derive(Serialize, Deserialize)]
struct RelKeyJson {
    version: VersionJson,
    keys: KeysJson,
}

#[derive(Serialize, Deserialize)]
struct FileInformationJson {
    fname: String,
    fhash: String,
    fsize: String,
    fowner: String,
    fpermissions: String,
    fmodified: String,
    fchecksum: String,
}

#[derive(Serialize, Deserialize)]
struct BlockJson {
    blockid: String,
    bchecksum: String,
    blocksize: String,
    filepos: String,
    blockaid: String,
    blockapos: String,
}

#[derive(Serialize, Deserialize)]
struct RelFileJson {
    version: VersionJson,
    fileinformation: FileInformationJson,
    blocks: Vec<BlockJson>,
}

fn parse_num<T: std::str::FromStr>(field: &str, v: &str) -> Result<T, String> {
    v.parse::<T>()
        .map_err(|_| format!("invalid value for '{}': {:?}", field, v))
}

impl BlockJson {
    fn from_block(fb: &BlockInformation) -> Self {
        BlockJson {
            blockid: fb.blockid.to_string(),
            bchecksum: fb.bchecksum.clone(),
            blocksize: fb.blocksize.to_string(),
            filepos: fb.filepos.to_string(),
            blockaid: fb.blockaid.clone(),
            blockapos: fb.blockapos.to_string(),
        }
    }

    fn to_block(&self) -> Result<BlockInformation, String> {
        if !is_hex_id(&self.blockaid) {
            return Err(format!("invalid blockaid: {:?}", self.blockaid));
        }
        Ok(BlockInformation {
            blockid: parse_num("blockid", &self.blockid)?,
            bchecksum: self.bchecksum.clone(),
            blocksize: parse_num("blocksize", &self.blocksize)?,
            filepos: parse_num("filepos", &self.filepos)?,
            blockaid: self.blockaid.clone(),
            blockapos: parse_num("blockapos", &self.blockapos)?,
        })
    }
}

impl FileInformationJson {
    fn from_fileinformation(fi: &FileInformation) -> Self {
        FileInformationJson {
            fname: fi.fname.clone(),
            fhash: fi.fhash.clone(),
            fsize: fi.fsize.to_string(),
            fowner: fi.fowner.clone(),
            fpermissions: format!("{:o}", fi.fpermissions),
            fmodified: fi.fmodified.clone(),
            fchecksum: fi.fchecksum.clone(),
        }
    }

    fn to_fileinformation(&self) -> Result<FileInformation, String> {
        Ok(FileInformation {
            fname: self.fname.clone(),
            fhash: self.fhash.clone(),
            fowner: self.fowner.clone(),
            fsize: parse_num("fsize", &self.fsize)?,
            fpermissions: u64::from_str_radix(&self.fpermissions, 8).map_err(|_| {
                format!("invalid value for 'fpermissions': {:?}", self.fpermissions)
            })?,
            fmodified: self.fmodified.clone(),
            fchecksum: self.fchecksum.clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// JSON reading helpers
// ---------------------------------------------------------------------------

/// Parse a relfiles JSON document into its file information and blocks.
/// Every `blockaid` must be a 64-char hex id.
pub fn parse_relfile_json(json: &str) -> Result<(FileInformation, Vec<BlockInformation>), String> {
    let doc: RelFileJson =
        serde_json::from_str(json).map_err(|e| format!("invalid relfiles JSON: {}", e))?;
    let fi = doc.fileinformation.to_fileinformation()?;
    let blocks = doc
        .blocks
        .iter()
        .map(BlockJson::to_block)
        .collect::<Result<Vec<_>, _>>()?;
    Ok((fi, blocks))
}

/// Parse block objects from the "blocks" array in the relfiles JSON.
pub fn parse_blocks_json(json: &str) -> Result<Vec<BlockInformation>, String> {
    parse_relfile_json(json).map(|(_, blocks)| blocks)
}

/// Read file information and blocks from relfiles/<3rd_byte>/<fhash>.json
pub fn read_relfile_json(
    db_base: &Path,
    fhash: &str,
) -> Result<(FileInformation, Vec<BlockInformation>), String> {
    if !is_hex_id(fhash) {
        return Err(format!("invalid fhash: {:?}", fhash));
    }
    let relfiles_dir = db_base.join("relfiles").join(hash_third_byte(fhash));
    let path = safe_json_path(&relfiles_dir, fhash);
    let content =
        fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    let (fi, blocks) =
        parse_relfile_json(&content).map_err(|e| format!("{}: {}", path.display(), e))?;
    if fi.fhash != fhash {
        return Err(format!(
            "{}: fhash {} does not match the file name",
            path.display(),
            fi.fhash
        ));
    }
    Ok((fi, blocks))
}

/// Read block information from relfiles/<3rd_byte>/<fhash>.json
pub fn read_blocks_json(db_base: &Path, fhash: &str) -> Result<Vec<BlockInformation>, String> {
    read_relfile_json(db_base, fhash).map(|(_, blocks)| blocks)
}

/// All file hashes that have a relfile below `db_base`, sorted.
pub fn list_relfile_hashes(db_base: &Path) -> Vec<String> {
    let subdirs = fs::read_dir(db_base.join("relfiles")).into_iter().flatten();
    let mut hashes: Vec<String> = subdirs
        .flatten()
        .flat_map(|d| fs::read_dir(d.path()).into_iter().flatten().flatten())
        .filter_map(|f| {
            let name = f.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".json").map(str::to_string)
        })
        .filter(|h| is_hex_id(h))
        .collect();
    hashes.sort();
    hashes
}

/// Read key information from relkeys/<3rd_byte>/.../<aid>.json
/// The key file must not be accessible by group or others.
pub fn read_key_json(db_base: &Path, aid: &str) -> Result<KeyInformation, String> {
    if !is_hex_id(aid) {
        return Err(format!("invalid aid: {:?}", aid));
    }
    let relkeys_dir = db_base.join("relkeys").join(hash_third_byte(aid));
    let path = safe_json_path(&relkeys_dir, aid);
    check_private_file(&path)?;
    let mut content =
        fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    let doc: Result<RelKeyJson, _> = serde_json::from_str(&content);
    content.zeroize();
    let doc = doc.map_err(|e| format!("{}: invalid relkeys JSON: {}", path.display(), e))?;
    let ivec = Key128::from_hex(&doc.keys.ivec).map_err(|e| format!("ivec: {}", e))?;
    let pkey = Key256::from_hex(&doc.keys.pkey).map_err(|e| format!("pkey: {}", e))?;
    let localnchunks = parse_num("localnchunks", &doc.keys.localnchunks)?;
    Ok(KeyInformation {
        ivec,
        pkey,
        localnchunks,
    })
}

// ---------------------------------------------------------------------------
// JSON writing helpers
// ---------------------------------------------------------------------------

/// Format a relkeys JSON document.
pub fn format_key_json(ki: &KeyInformation) -> String {
    let doc = RelKeyJson {
        version: VersionJson::current(),
        keys: KeysJson {
            ivec: ki.ivec.to_hex(),
            pkey: ki.pkey.to_hex(),
            localnchunks: ki.localnchunks.to_string(),
        },
    };
    serde_json::to_string_pretty(&doc).expect("serialising strings cannot fail")
}

/// Format a relfiles JSON document; blocks are written in the given order.
pub fn format_relfile_json(fi: &FileInformation, blocks: &[&BlockInformation]) -> String {
    let doc = RelFileJson {
        version: VersionJson::current(),
        fileinformation: FileInformationJson::from_fileinformation(fi),
        blocks: blocks.iter().map(|fb| BlockJson::from_block(fb)).collect(),
    };
    serde_json::to_string_pretty(&doc).expect("serialising strings cannot fail")
}

/// Build a path that avoids exceeding the 255-byte filename limit.
fn safe_path(base: &Path, name: &str, ext: &str) -> Result<PathBuf, String> {
    let filename = format!("{}.{}", name, ext);
    if filename.len() <= 255 {
        Ok(base.join(filename))
    } else {
        let split = name.len().saturating_sub(8);
        let prefix = &name[..split];
        let suffix = &name[split..];
        let dir = base.join(prefix);
        ensure_private_dir(&dir)?;
        Ok(dir.join(format!("{}.{}", suffix, ext)))
    }
}

/// Write `json` to `<dir>/<name>.json` with mode 0600, directories 0700.
fn write_private_json(dir: &Path, name: &str, json: &str) -> Result<(), String> {
    ensure_private_dir(dir)?;
    let path = safe_path(dir, name, "json")?;
    write_file_mode(&path, format!("{}\n", json).as_bytes(), PRIVATE_FILE_MODE)
        .map_err(|e| format!("cannot write {}: {}", path.display(), e))
}

/// Write one JSON file per assembly key: <db_path>/<myid>/relkeys/<3rd_byte>/<aid>.json
/// Key files are created with mode 0600, `<db_path>/<myid>` and the
/// directories below it with mode 0700.
pub fn output_rel_keys_json(proc: &Processor, dryrun: bool) -> Result<(), String> {
    let entries = &proc.cache.ackstore.entries;
    if dryrun {
        for (_aid, ki) in entries {
            println!("{}", format_key_json(ki));
        }
        return Ok(());
    }
    let mybase = Path::new(&proc.config.path_db).join(&proc.config.my_id);
    ensure_private_dir(&mybase)?;
    let base = mybase.join("relkeys");
    for (aid, ki) in entries {
        let dir = base.join(hash_third_byte(aid));
        let mut json = format_key_json(ki);
        let res = write_private_json(&dir, aid, &json);
        json.zeroize();
        res?;
    }
    Ok(())
}

/// Group the blocks by file hash. Per file the blocks are sorted by file
/// position and numbered from 1 (elykseer-ml `Env.consolidate_files`);
/// files are sorted by hash.
pub fn consolidate_files(
    entries: &[(String, BlockInformation)],
) -> Vec<(String, Vec<BlockInformation>)> {
    let mut map: std::collections::BTreeMap<&str, Vec<BlockInformation>> =
        std::collections::BTreeMap::new();
    for (key, val) in entries {
        map.entry(key).or_default().push(val.clone());
    }
    map.into_iter()
        .map(|(fhash, mut bis)| {
            bis.sort_by_key(|b| b.filepos); // stable
            // a file backed up twice in one run: the block added last wins
            bis.reverse();
            bis.dedup_by_key(|b| b.filepos);
            bis.reverse();
            for (i, b) in bis.iter_mut().enumerate() {
                b.blockid = i as u32 + 1;
            }
            (fhash.to_string(), bis)
        })
        .collect()
}

/// Write one JSON file per backed-up file: <db_path>/<myid>/relfiles/<3rd_byte>/<fhash>.json
/// Files are created with mode 0600, directories with mode 0700.
/// Blocks without file information belong to a file whose backup failed:
/// no metadata is written for it and an error is returned after the other
/// files have been written.
pub fn output_rel_files_json(proc: &Processor, dryrun: bool) -> Result<(), String> {
    let fiset = &proc.cache.acfistore.entries;
    let fbis = consolidate_files(&proc.cache.acfbstore.entries);

    if dryrun {
        println!();
        for (fhash, bis) in &fbis {
            if let Some(fi) = fiset
                .iter()
                .find(|(_, fi)| fi.fhash == *fhash)
                .map(|(_, fi)| fi)
            {
                println!("fhash = {}", fhash);
                println!(
                    "  fname={} fsize={} fowner={} fpermissions={:o} fmodified={} fchecksum={}",
                    fi.fname, fi.fsize, fi.fowner, fi.fpermissions, fi.fmodified, fi.fchecksum
                );
            }
            for fb in bis.iter().rev() {
                println!(
                    "  block {}:{}@{} checksum={} aid={} apos={}",
                    fb.blockid, fb.blocksize, fb.filepos, fb.bchecksum, fb.blockaid, fb.blockapos
                );
            }
        }
        Ok(())
    } else {
        let mybase = Path::new(&proc.config.path_db).join(&proc.config.my_id);
        ensure_private_dir(&mybase)?;
        let base = mybase.join("relfiles");
        let mut missing = Vec::new();
        for (fhash, bis) in &fbis {
            // the file information added last is the current one
            let Some((_, fi)) = fiset.iter().rev().find(|(_, fi)| fi.fhash == *fhash) else {
                missing.push(fhash.as_str());
                continue;
            };
            let dir = base.join(hash_third_byte(fhash));
            let bis: Vec<&BlockInformation> = bis.iter().collect();
            write_private_json(&dir, fhash, &format_relfile_json(fi, &bis))?;
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "no file information for blocks of file hash {}; meta data not written",
                missing.join(", ")
            ))
        }
    }
}

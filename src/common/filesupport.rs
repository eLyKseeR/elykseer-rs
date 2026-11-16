use std::fs;
use std::path::Path;

use crate::common::configuration::Configuration;
use crate::common::utilities::{file_hash, sha3_256_file};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInformation {
    pub fname: String,
    pub fhash: String,
    pub fowner: String,
    pub fsize: u64,
    pub fpermissions: u64,
    pub fmodified: String,
    pub fchecksum: String,
}

/// Convert Unix epoch seconds to a UTC datetime string "YYYY-MM-DD HH:MM:SS".
/// Uses Howard Hinnant's civil-from-days algorithm (public domain).
fn format_datetime_utc(secs: u64) -> String {
    let time = secs % 86400;
    let h = time / 3600;
    let m = (time % 3600) / 60;
    let s = time % 60;

    let z = (secs / 86400) as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr = if mo <= 2 { y + 1 } else { y };

    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", yr, mo, d, h, m, s)
}

/// Coq: get_file_information
/// Gather file metadata and compute a hash-based identifier
/// (`fhash` = SHA3-256 of `fname ^ my_id`). `fchecksum` is the SHA3-256
/// of the file content, as in elykseer-ml; it is empty if the file cannot
/// be read.
pub fn get_file_information(config: &Configuration, fname: &str) -> FileInformation {
    let path = Path::new(fname);
    let (fsize, fpermissions, fmodified, fowner) = if let Ok(meta) = fs::metadata(path) {
        let sz = meta.len();
        #[cfg(unix)]
        let (perms, owner) = {
            use std::os::unix::fs::MetadataExt;
            use std::os::unix::fs::PermissionsExt;
            let mode = meta.permissions().mode();
            (u64::from(mode & 0o777), meta.uid().to_string())
        };
        #[cfg(not(unix))]
        let (perms, owner) = {
            let p = if meta.permissions().readonly() {
                0o444u64
            } else {
                0o644u64
            };
            (p, String::new())
        };
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| format_datetime_utc(d.as_secs()))
            })
            .unwrap_or_default();
        (sz, perms, modified, owner)
    } else {
        (0, 0, String::new(), String::new())
    };
    let fhash = file_hash(fname, &config.my_id);
    let fchecksum = sha3_256_file(path).unwrap_or_default();
    FileInformation {
        fname: fname.to_string(),
        fhash,
        fowner,
        fsize,
        fpermissions,
        fmodified,
        fchecksum,
    }
}

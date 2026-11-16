use crate::common::fsutils::PRIVATE_FILE_MODE;
use crate::common::nchunks::Nchunks;
use crate::common::tracer::Tracer;

pub struct Configuration {
    /// Coq: config_nchunks : Nchunks.t
    pub config_nchunks: Nchunks,
    pub path_chunks: String,
    pub path_db: String,
    pub my_id: String,
    pub trace: Tracer,
    /// Permission bits of chunk files (Unix), default 0o600.
    pub chunk_mode: u32,
}

impl Configuration {
    /// `config_nchunks` is clamped to the range of `Nchunks` (16..=256).
    pub fn new(
        config_nchunks: u32,
        path_chunks: String,
        path_db: String,
        my_id: String,
        trace: Tracer,
    ) -> Self {
        Configuration {
            config_nchunks: Nchunks::new(config_nchunks),
            path_chunks,
            path_db,
            my_id,
            trace,
            chunk_mode: PRIVATE_FILE_MODE,
        }
    }

    /// Set the permission bits of chunk files (e.g. 0o644 to share them).
    pub fn with_chunk_mode(mut self, mode: u32) -> Self {
        self.chunk_mode = mode & 0o777;
        self
    }
}

/// Default database path: `$XDG_DATA_HOME/elykseer`, or
/// `$HOME/.local/share/elykseer` if `XDG_DATA_HOME` is unset or not absolute.
/// A persistent per-user location, unlike `$TMPDIR`, which may be shared and
/// is cleaned on reboot (and with it the keys).
pub fn default_db_path() -> Option<String> {
    let from_env = |var: &str| {
        std::env::var_os(var)
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    let base = from_env("XDG_DATA_HOME")
        .or_else(|| from_env("HOME").map(|h| h.join(".local").join("share")))?;
    Some(base.join("elykseer").to_string_lossy().to_string())
}

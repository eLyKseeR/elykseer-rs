use std::path::Path;

// ---------------------------------------------------------------------------
// File and directory creation with explicit permissions
// ---------------------------------------------------------------------------

/// Mode of key and metadata files: owner read/write only.
pub const PRIVATE_FILE_MODE: u32 = 0o600;
/// Mode of the key and metadata directories: owner only.
pub const PRIVATE_DIR_MODE: u32 = 0o700;

/// Directory mode matching a file mode: add `x` wherever `r` is set
/// (0o600 → 0o700, 0o644 → 0o755).
pub fn dir_mode_for(file_mode: u32) -> u32 {
    let m = file_mode & 0o777;
    m | ((m & 0o444) >> 2)
}

/// Create `path` and its missing parents with `mode` (Unix; the umask still
/// applies). Existing directories keep their mode.
pub fn create_dir_all_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(mode);
    }
    #[cfg(not(unix))]
    let _ = mode;
    builder.create(path)
}

/// Write `data` to `path`, creating or truncating it, with permission bits
/// `mode` (Unix). The mode is also set on an existing file, so a key file
/// that was readable by others before becomes private.
pub fn write_file_mode(path: &Path, data: &[u8], mode: u32) -> std::io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(mode);
    }
    let mut f = opts.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(std::fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    f.write_all(data)
}

/// Like `write_file_mode`, but fails if `path` already exists.
pub fn write_new_file_mode(path: &Path, data: &[u8], mode: u32) -> std::io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(mode);
    }
    #[cfg(not(unix))]
    let _ = mode;
    let mut f = opts.open(path)?;
    f.write_all(data)
}

/// Fail if `path` can be read or written by group or others (like ssh does
/// for private keys). Always succeeds on non-Unix systems.
pub fn check_private_file(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(path)
            .map_err(|e| format!("cannot stat {}: {}", path.display(), e))?;
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(format!(
                "permissions {:o} for '{}' are too open; it must not be accessible by group or others (chmod 600)",
                mode,
                path.display()
            ));
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

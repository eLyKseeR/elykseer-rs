use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::Arc;

use elykseer_lib::assembly::BlockInformation;
use elykseer_lib::common::configuration::{Configuration, default_db_path};
use elykseer_lib::common::filesupport::FileInformation;
use elykseer_lib::common::metadata::{list_relfile_hashes, read_key_json, read_relfile_json};
use elykseer_lib::common::tracer::Tracer;
use elykseer_lib::common::utilities::{file_hash, rnd256, sha3_256_file};
use elykseer_lib::nchunks::Nchunks;
use elykseer_lib::processor::Processor;

// Exit codes: 0 all files restored (or verified) completely, 1 a file
// failed, 2 usage error or no database.

/// Metadata of a file, or why it could not be read.
type Relation = Result<(FileInformation, Vec<BlockInformation>), String>;

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

struct Args {
    verbose: bool,
    verify: bool,
    chunk_path: String,
    db_path: String,
    out_path: String,
    my_id: Option<String>,
    files: Vec<String>,
}

fn print_usage() {
    eprintln!("lxr_restore -i <id> [options] <file> ...");
    eprintln!("lxr_restore -i <id> --verify [options] [<file> ...]");
    eprintln!("  -i <id>       own identifier (required)");
    eprintln!("  -x <path>     path for encrypted chunks (default: lxr)");
    eprintln!(
        "  -d <path>     database path (default: $XDG_DATA_HOME/elykseer or ~/.local/share/elykseer)"
    );
    eprintln!("  -o <path>     output path for restored files (default: $TMPDIR)");
    eprintln!("  --verify      restore into a temporary directory, compare each file with its");
    eprintln!("                checksum from the metadata, and remove it; without file names:");
    eprintln!("                all files of the identifier");
    eprintln!("  -v            verbose output");
    eprintln!("  -h, --help    show this help");
    eprintln!("  --version     show the version");
}

fn usage_error(msg: &str) -> ! {
    eprintln!("{}", msg);
    print_usage();
    exit(2)
}

impl Args {
    fn parse() -> Self {
        let mut args = Args {
            verbose: false,
            verify: false,
            chunk_path: "lxr".to_string(),
            db_path: default_db_path().unwrap_or_default(),
            out_path: std::env::temp_dir().to_string_lossy().to_string(),
            my_id: None,
            files: Vec::new(),
        };
        let mut iter = std::env::args().skip(1);
        while let Some(arg) = iter.next() {
            let mut value = || {
                iter.next()
                    .unwrap_or_else(|| usage_error(&format!("option {} needs a value", arg)))
            };
            match arg.as_str() {
                "-v" => args.verbose = true,
                "--verify" => args.verify = true,
                "-x" => args.chunk_path = value(),
                "-o" => args.out_path = value(),
                "-d" => args.db_path = value(),
                "-i" => args.my_id = Some(value()),
                "-n" => {
                    value();
                    eprintln!(
                        "lxr_restore: parameter -n is obsolete and ignored, the number of chunks is read from the key information"
                    );
                }
                "-h" | "--help" => {
                    print_usage();
                    exit(0);
                }
                "--version" => {
                    println!("lxr_restore {}", env!("CARGO_PKG_VERSION"));
                    exit(0);
                }
                other if other.starts_with('-') => {
                    usage_error(&format!("unknown option: {}", other))
                }
                other => args.files.push(other.to_string()),
            }
        }
        args
    }
}

// ---------------------------------------------------------------------------
// Restore logic
// ---------------------------------------------------------------------------

/// The path of a restored file relative to the output directory: a leading
/// `/` is dropped (an absolute name is restored below the output directory,
/// like tar), names with `..` are refused.
fn restore_relpath(fname: &str) -> Result<String, String> {
    let parts: Vec<&str> = fname
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    if parts.contains(&"..") {
        Err(format!("file name '{}' contains '..'", fname))
    } else if parts.is_empty() {
        Err(format!("file name '{}' is empty", fname))
    } else {
        Ok(parts.join("/"))
    }
}

/// True if `target` stays below `basep` also when following the symlinks
/// that already exist on its way.
fn resolves_below(basep: &Path, target: &Path) -> bool {
    let Ok(cbase) = basep.canonicalize() else {
        return false;
    };
    // the deepest existing directory decides
    target
        .ancestors()
        .skip(1)
        .find_map(|a| a.canonicalize().ok())
        .is_some_and(|c| c.starts_with(&cbase))
}

/// Load the keys of all assemblies the files refer to into the key store
/// of the assembly cache.
fn ensure_keys_available(
    mut proc: Processor,
    db_base: &Path,
    rels: &[(String, Relation)],
) -> Processor {
    let aids: BTreeSet<&str> = rels
        .iter()
        .filter_map(|(_, rel)| rel.as_ref().ok())
        .flat_map(|(_, blocks)| blocks.iter().map(|bi| bi.blockaid.as_str()))
        .collect();
    for aid in aids {
        match read_key_json(db_base, aid) {
            Ok(ki) => proc.cache = proc.cache.add_key(aid, ki),
            // the files of this assembly fail to restore and are reported there
            Err(e) => eprintln!("  key not available for assembly {}: {}", aid, e),
        }
    }
    proc
}

/// Restore one file. Returns the bytes restored and whether it succeeded;
/// a file that could not be restored completely is removed again.
fn restore_file(
    proc: Processor,
    basep: &Path,
    fname: &str,
    rel: &Relation,
    verify: bool,
    verbose: bool,
) -> (u64, bool, Processor) {
    let refuse = |proc, msg: &str| {
        eprintln!("  cannot restore file '{}': {}", fname, msg);
        (0, false, proc)
    };
    let (fi, blocks) = match rel {
        Ok(rel) => rel,
        Err(_) if !verbose => return refuse(proc, "no meta data found"),
        Err(e) => return refuse(proc, &format!("no meta data found ({})", e)),
    };
    let relp = match restore_relpath(fname) {
        Ok(relp) => relp,
        Err(msg) => return refuse(proc, &msg),
    };
    let target = basep.join(&relp);
    if !resolves_below(basep, &target) {
        return refuse(proc, "it resolves outside of the output directory");
    }
    if target.symlink_metadata().is_ok() {
        return refuse(proc, &format!("'{}' already exists", target.display()));
    }

    let (n, proc) = proc.file_restore(basep, Path::new(&relp), blocks);

    if n != fi.fsize {
        // missing blocks: e.g. lost chunks, wrong key, or corrupted data
        let _ = std::fs::remove_file(&target);
        eprintln!(
            "  failed to restore file '{}': restored {} of {} bytes",
            fname, n, fi.fsize
        );
        return (0, false, proc);
    }
    if verify {
        // the blocks were verified already, this checks the file as a whole
        let ok = sha3_256_file(&target).is_ok_and(|c| c == fi.fchecksum);
        let _ = std::fs::remove_file(&target);
        if !ok {
            eprintln!("  failed to verify file '{}': checksum differs", fname);
            return (0, false, proc);
        }
        if verbose {
            println!("  verified file '{}'", fname);
        }
    } else if verbose {
        println!("  restored file '{}': {} bytes", fname, n);
    }
    (n, true, proc)
}

/// A new private directory below the system's temp directory.
fn make_temp_dir() -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("lxr_verify_{}", &rnd256("")[..16]));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&dir)?; // not recursive: fails if it exists
    Ok(dir)
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    let args = Args::parse();

    if args.files.is_empty() && !args.verify {
        println!("nothing to do.");
        print_usage();
        return;
    }
    let Some(my_id) = args.my_id.clone() else {
        usage_error("own identifier missing: -i <id> is required.");
    };
    if args.db_path.is_empty() {
        usage_error("no default database path ($HOME not set): -d <path> is required.");
    }
    if !Path::new(&args.db_path).is_dir() {
        eprintln!("no meta data database found at '{}'", args.db_path);
        exit(2);
    }
    if !args.verify && !Path::new(&args.out_path).is_dir() {
        eprintln!(
            "output directory '{}' does not exist or is not a directory",
            args.out_path
        );
        exit(2);
    }

    let tracer = if args.verbose {
        Tracer::stdout_tracer_debug()
    } else {
        Tracer::stdout_tracer_warning()
    };

    let config = Arc::new(Configuration::new(
        // restore reads the number of chunks from the key information
        Nchunks::MIN,
        args.chunk_path.clone(),
        args.db_path.clone(),
        my_id.clone(),
        tracer,
    ));

    let db_base = Path::new(&args.db_path).join(&my_id);

    // the metadata of each file is read only once
    let rels: Vec<(String, Relation)> = if args.files.is_empty() {
        // --verify without names: all files of this identifier
        list_relfile_hashes(&db_base)
            .iter()
            .map(|fhash| match read_relfile_json(&db_base, fhash) {
                Ok(rel) => (rel.0.fname.clone(), Ok(rel)),
                Err(e) => (format!("<file hash {}>", fhash), Err(e)),
            })
            .collect()
    } else {
        args.files
            .iter()
            .map(|fname| {
                let fhash = file_hash(fname, &my_id);
                (fname.clone(), read_relfile_json(&db_base, &fhash))
            })
            .collect()
    };

    let basep = if args.verify {
        match make_temp_dir() {
            Ok(dir) => dir,
            Err(e) => {
                eprintln!("cannot create a temporary directory: {}", e);
                exit(1);
            }
        }
    } else {
        PathBuf::from(&args.out_path)
    };

    let mut proc = ensure_keys_available(Processor::prepare(config), &db_base, &rels);

    let nf = rels.len();
    let mut nok = 0;
    let mut total_bytes = 0u64;
    for (fname, rel) in &rels {
        let (n, ok, p) = restore_file(proc, &basep, fname, rel, args.verify, args.verbose);
        total_bytes += n;
        nok += usize::from(ok);
        proc = p;
    }

    if args.verify {
        let _ = std::fs::remove_dir_all(&basep);
    }

    let nfailed = nf - nok;
    if args.verbose || nfailed > 0 {
        println!(
            "  {} {} of {} files with {} bytes in total",
            if args.verify { "verified" } else { "restored" },
            nok,
            nf,
            total_bytes
        );
    }
    if nfailed > 0 {
        exit(1);
    }
    println!("done.");
}

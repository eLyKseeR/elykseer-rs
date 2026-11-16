use std::collections::BTreeSet;
use std::path::Path;
use std::process::exit;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use elykseer_lib::common::configuration::{Configuration, default_db_path};
use elykseer_lib::common::fsutils::{
    PRIVATE_DIR_MODE, PRIVATE_FILE_MODE, create_dir_all_mode, dir_mode_for,
};
use elykseer_lib::common::metadata::{
    output_rel_files_json, output_rel_keys_json, read_key_json, read_relfile_json,
};
use elykseer_lib::common::tracer::{LogFn, LogLevel, Tracer};
use elykseer_lib::common::utilities::file_hash;
use elykseer_lib::nchunks::Nchunks;
use elykseer_lib::processor::Processor;

// Exit codes: 0 all files backed up, 1 a file or its metadata failed,
// 2 usage error.

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

struct Args {
    verbose: bool,
    dryrun: bool,
    chunk_path: String,
    db_path: String,
    nchunks: Nchunks,
    my_id: Option<String>,
    chunk_mode: u32,
    recursive: bool,
    directory: String,
    files: Vec<String>,
}

fn print_usage() {
    eprintln!("lxr_backup -i <id> [options] [-D <dir> [-R]] [<file> ...]");
    eprintln!("  -i <id>       own identifier (required)");
    eprintln!("  -x <path>     output path for encrypted chunks (default: lxr)");
    eprintln!(
        "  -d <path>     database path (default: $XDG_DATA_HOME/elykseer or ~/.local/share/elykseer)"
    );
    eprintln!("  -n <int>      number of chunks 16-256 per assembly (default: 16)");
    eprintln!("  -m <octal>    permissions of chunk files (default: 600)");
    eprintln!("  -D <dir>      directory to backup");
    eprintln!("  -R            recursively backup directory");
    eprintln!("  -y            dry run: print the metadata instead of writing it");
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
            dryrun: false,
            chunk_path: "lxr".to_string(),
            db_path: default_db_path().unwrap_or_default(),
            nchunks: Nchunks::new(Nchunks::MIN),
            my_id: None,
            chunk_mode: PRIVATE_FILE_MODE,
            recursive: false,
            directory: String::new(),
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
                "-y" => args.dryrun = true,
                "-R" => args.recursive = true,
                "-x" => args.chunk_path = value(),
                "-d" => args.db_path = value(),
                "-i" => args.my_id = Some(value()),
                "-D" => args.directory = value(),
                "-n" => {
                    let v = value();
                    args.nchunks = v
                        .parse::<i32>()
                        .ok()
                        .and_then(|n| Nchunks::try_from(n).ok())
                        .unwrap_or_else(|| {
                            usage_error(&format!(
                                "-n must be between {} and {}, got {}",
                                Nchunks::MIN,
                                Nchunks::MAX,
                                v
                            ))
                        });
                }
                "-m" => match u32::from_str_radix(&value(), 8) {
                    Ok(m) if m <= 0o777 => args.chunk_mode = m,
                    _ => usage_error("-m expects an octal mode, e.g. 600 or 644"),
                },
                "-h" | "--help" => {
                    print_usage();
                    exit(0);
                }
                "--version" => {
                    println!("lxr_backup {}", env!("CARGO_PKG_VERSION"));
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
// Failure accounting
// ---------------------------------------------------------------------------

type Failures = Arc<AtomicUsize>;

fn fail(failures: &Failures, msg: &str) {
    failures.fetch_add(1, Ordering::Relaxed);
    eprintln!("{}", msg);
}

/// A tracer that prints like the stdout tracers and counts warnings and
/// errors: each of them means that something was not backed up.
fn counting_tracer(verbose: bool, failures: &Failures) -> Tracer {
    let plain = |level: LogLevel| -> LogFn {
        if verbose {
            Box::new(move |m| Tracer::output_stdout(level, m))
        } else {
            Box::new(|_| Some(()))
        }
    };
    let counted = |level: LogLevel| -> LogFn {
        let failures = Arc::clone(failures);
        Box::new(move |m| {
            failures.fetch_add(1, Ordering::Relaxed);
            Tracer::output_stdout(level, m)
        })
    };
    Tracer::new(
        plain(LogLevel::Debug),
        plain(LogLevel::Info),
        counted(LogLevel::Warning),
        counted(LogLevel::Error),
    )
}

// ---------------------------------------------------------------------------
// Backup helpers
// ---------------------------------------------------------------------------

/// A file that cannot be read is reported and skipped.
fn readable_file(fp: &Path) -> Result<(), String> {
    let meta = std::fs::metadata(fp).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("not a regular file".to_string());
    }
    std::fs::File::open(fp)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn run_file_backup(
    proc: Processor,
    db_base: &Path,
    filepath: &Path,
    failures: &Failures,
) -> Processor {
    if let Err(msg) = readable_file(filepath) {
        fail(
            failures,
            &format!("cannot backup file '{}': {}", filepath.display(), msg),
        );
        return proc;
    }
    // deduplication against the metadata of an earlier backup of this file
    let fhash = file_hash(&filepath.to_string_lossy(), &proc.config.my_id);
    let rel = read_relfile_json(db_base, &fhash).ok();
    let known = |fh: &str| rel.as_ref().filter(|_| fh == fhash);
    proc.file_backup(
        |fh| known(fh).map(|(fi, _)| fi.fchecksum.clone()),
        |fh| known(fh).map(|(_, bs)| bs.clone()).unwrap_or_default(),
        filepath,
    )
}

fn run_dir_backup(
    proc: Processor,
    db_base: &Path,
    dirpath: &Path,
    failures: &Failures,
) -> Processor {
    let (files, _dirs) = Processor::list_directory_entries(dirpath);
    files
        .0
        .iter()
        .fold(proc, |p, fp| run_file_backup(p, db_base, fp, failures))
}

fn recursive_dir_backup(
    proc: Processor,
    db_base: &Path,
    dirpath: &Path,
    failures: &Failures,
) -> Processor {
    println!("recurse into {}", dirpath.display());
    let proc = run_dir_backup(proc, db_base, dirpath, failures);
    let (_files, dirs) = Processor::list_directory_entries(dirpath);
    dirs.0
        .iter()
        .fold(proc, |p, dp| recursive_dir_backup(p, db_base, dp, failures))
}

/// Every block must refer to an assembly whose key is known, either from
/// this run or from the metadata of an earlier backup (deduplication).
fn check_keys(proc: &Processor, db_base: &Path, failures: &Failures) {
    let aids: BTreeSet<&str> = proc
        .cache
        .acfbstore
        .entries
        .iter()
        .map(|(_, bi)| bi.blockaid.as_str())
        .collect();
    for aid in aids {
        if proc.cache.ackstore.find(aid).is_none() && read_key_json(db_base, aid).is_err() {
            fail(
                failures,
                &format!("no key for assembly {}; its blocks cannot be restored", aid),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    let args = Args::parse();

    if args.files.is_empty() && args.directory.is_empty() {
        println!("no directory or no files to backup given in command line arguments.");
        print_usage();
        return;
    }
    let Some(my_id) = args.my_id.clone() else {
        usage_error("own identifier missing: -i <id> is required.");
    };
    if args.db_path.is_empty() {
        usage_error("no default database path ($HOME not set): -d <path> is required.");
    }

    let failures: Failures = Arc::new(AtomicUsize::new(0));

    let config = Arc::new(
        Configuration::new(
            args.nchunks.to_positive(),
            args.chunk_path.clone(),
            args.db_path.clone(),
            my_id.clone(),
            counting_tracer(args.verbose, &failures),
        )
        .with_chunk_mode(args.chunk_mode),
    );

    // ensure output directories exist (new directories are private)
    for (dir, mode) in [
        (&args.chunk_path, dir_mode_for(args.chunk_mode)),
        (&args.db_path, PRIVATE_DIR_MODE),
    ] {
        if let Err(e) = create_dir_all_mode(Path::new(dir), mode) {
            eprintln!("cannot create directory '{}': {}", dir, e);
            exit(1);
        }
    }
    let db_base = Path::new(&args.db_path).join(&my_id);

    let proc = Processor::prepare(Arc::clone(&config));

    let proc = if args.directory.is_empty() {
        // backup each file
        args.files.iter().fold(proc, |p, filename| {
            run_file_backup(p, &db_base, Path::new(filename), &failures)
        })
    } else if args.recursive {
        recursive_dir_backup(proc, &db_base, Path::new(&args.directory), &failures)
    } else {
        run_dir_backup(proc, &db_base, Path::new(&args.directory), &failures)
    };

    // close the processor — finalise and extract chunks
    let proc = proc.close();

    // output relations: keys first, a relfile is useless without them
    if !args.dryrun
        && let Err(e) = output_rel_keys_json(&proc, false)
    {
        fail(&failures, &e);
    }
    if let Err(e) = output_rel_files_json(&proc, args.dryrun) {
        fail(&failures, &e);
    }
    if !args.dryrun {
        check_keys(&proc, &db_base, &failures);
    }

    let nfailed = failures.load(Ordering::Relaxed);
    if nfailed > 0 {
        eprintln!(
            "backup incomplete: {} failure{}, see messages above",
            nfailed,
            if nfailed > 1 { "s" } else { "" }
        );
        exit(1);
    }
    println!("done.");
}

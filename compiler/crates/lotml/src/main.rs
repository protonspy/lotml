//! The `lotml` command: one binary for checking, explaining, formatting, building and testing.
//!
//! Exit status: 0 when nothing is wrong, 1 when the files have errors (or a test failed), 2 when
//! the command could not run.

mod check;
mod dev;
mod exec;
mod files;
mod guide;
mod index;
mod init;
mod lsp;
mod mcp;
mod rpc;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "lotml", version, about = "The lotml compiler")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check files for syntax, type and mutability errors.
    Check {
        /// Files, or directories searched for `.lotml` files.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// How to print the diagnostics.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Shorthand for `--format json`.
        #[arg(long)]
        json: bool,
        /// Show every diagnostic, not only the first five.
        #[arg(long)]
        all: bool,
        /// Apply the fixes that are safe without review, then report what is left.
        #[arg(long)]
        fix: bool,
        /// Report only the diagnostics introduced since this git revision.
        #[arg(long, value_name = "REV")]
        since: Option<String>,
        /// Treat each file as a prefix still being written: completable, error, or unknown.
        #[arg(long)]
        prefix: bool,
    },
    /// Explain an error code: `lotml explain E0204`.
    Explain {
        /// The code, as a diagnostic shows it.
        code: String,
    },
    /// Rewrite files in the canonical form.
    Fmt {
        /// Files, or directories searched for `.lotml` files.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Change nothing; exit 1 if a file is not in the canonical form.
        #[arg(long)]
        check: bool,
    },
    /// Print the types and the documented signatures: the index of a project, without bodies.
    Digest {
        /// Files, or directories searched for `.lotml` files.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Print a symbol as written, `name` or `Type.method`, and the functions and types it uses.
    Show {
        /// The symbol.
        symbol: String,
        /// Files, or directories searched for `.lotml` files.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Compile files to Python modules, written with the runtime they import, or to executables.
    Build {
        /// Files, or directories searched for `.lotml` files.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Where to write the modules.
        #[arg(short, long, default_value = "build")]
        out: PathBuf,
        /// What to compile to.
        #[arg(long, value_enum, default_value = "python")]
        target: Target,
    },
    /// Run a program's `fn main()`.
    Run {
        /// The program.
        path: PathBuf,
        /// What to compile to and run.
        #[arg(long, value_enum, default_value = "python")]
        target: Target,
    },
    /// Run the `test` blocks, reporting the values a failed comparison saw.
    Test {
        /// Files, or directories searched for `.lotml` files.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Report as JSON.
        #[arg(long)]
        json: bool,
        /// What to compile to and run.
        #[arg(long, value_enum, default_value = "python")]
        target: Target,
    },
    /// Write the interface lotml imports a Python module through, from the module's stub.
    Bind {
        /// The Python module: `textwrap`, `os.path`.
        module: String,
        /// The stub to read; typeshed's, from an installed mypy or jedi, when absent.
        #[arg(long)]
        stub: Option<PathBuf>,
        /// Where to write `<module>.lotmli`.
        #[arg(long, default_value = "bindings")]
        out: PathBuf,
    },
    /// Set a project up for coding agents: AGENTS.md, the guide, and the MCP server in each harness.
    Init {
        /// The project's root.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// The harnesses to set up, `claude,codex,cursor` or `none`, instead of the checklist.
        #[arg(long, value_enum, value_delimiter = ',')]
        harness: Option<Vec<init::Named>>,
        /// Set up the harnesses found in the project, without the checklist.
        #[arg(long)]
        yes: bool,
    },
    /// Serve the Language Server Protocol on standard input and output.
    Lsp,
    /// Serve the compiler's tools to an agent over MCP, on standard input and output.
    Mcp {
        /// The project: every `.lotml` file under it.
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Commands that build the harness guide's data.
    #[command(subcommand, hide = true)]
    Dev(Dev),
    /// The harness guide: what it is asked, and its answer.
    #[command(subcommand)]
    Guide(Guide),
}

#[derive(Subcommand)]
enum Guide {
    /// Ask the guide where to change the code, as the MCP tool asks it, and print its answer.
    Ask {
        /// The project: every `.lotml` file under it.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// What you are doing, in a sentence or two.
        #[arg(long)]
        task: Option<String>,
        /// Files inside the project; all of them when absent.
        files: Vec<String>,
    },
    /// Print the messages the guide is asked with, for a state given as JSON.
    Render {
        /// The state: `task`, `path`, `text`, and `diagnostics` or `failing`.
        state: PathBuf,
    },
}

#[derive(Subcommand)]
enum Dev {
    /// Say where a change between two versions of a file falls, and which one edit makes it.
    Diff {
        /// The first version.
        before: PathBuf,
        /// The second version.
        after: PathBuf,
        /// The file's path in the project, written into the edit's arguments.
        #[arg(long)]
        path: String,
        /// Print it as JSON.
        #[arg(long)]
        json: bool,
    },
    /// List a file's mutants: each a span replaced, with its operator and declaration.
    Mutate {
        /// The file.
        path: PathBuf,
        /// Print them as JSON, each with the mutated text.
        #[arg(long)]
        json: bool,
    },
}

/// What a program is compiled to: Python modules run by Python, or C built by a C compiler
/// (adr:0014).
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Target {
    Python,
    C,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Text,
    Json,
    Sarif,
}

/// Why a command could not run.
pub struct Failure(pub String);

/// The compiler recurses over nested syntax; a deep file is bounded by the parser's depth guard,
/// but the bound is generous, so the work runs on a thread with a stack large enough for it
/// rather than the platform default (1 MB on Windows).
const STACK: usize = 256 * 1024 * 1024;

fn main() -> ExitCode {
    // Where the stack cannot be reserved (a tightly limited container), run on this thread: the
    // depth guard still bounds the recursion, only less of it fits.
    match std::thread::Builder::new().stack_size(STACK).spawn(run) {
        Ok(worker) => worker.join().unwrap_or(ExitCode::from(2)),
        Err(_) => run(),
    }
}

fn run() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Check { paths, format, json, all, fix, since, prefix } => {
            let format = if json { Format::Json } else { format };
            check::run(&check::Options { paths, format, all, fix, since, prefix })
        }
        Command::Explain { code } => explain(&code),
        Command::Fmt { paths, check } => fmt(&paths, check),
        Command::Digest { paths } => sources(&paths).map(|s| {
            print!("{}", index::digest(&s));
            true
        }),
        Command::Build { paths, out, target } => return status(exec::build(&paths, &out, target)),
        Command::Run { path, target } => return status(exec::run(&path, target)),
        Command::Test { paths, json, target } => return status(exec::test(&paths, json, target)),
        Command::Bind { module, stub, out } => exec::bind(&module, stub.as_deref(), &out),
        Command::Init { dir, harness, yes } => return status(init::run(&dir, harness.as_deref(), yes)),
        Command::Lsp => return status(lsp::serve()),
        Command::Mcp { root } => return status(mcp::serve(&root)),
        Command::Dev(Dev::Mutate { path, json }) => dev::mutate(&path, json),
        Command::Dev(Dev::Diff { before, after, path, json }) => dev::diff(&before, &after, &path, json),
        Command::Guide(Guide::Render { state }) => guide::render_file(&state),
        Command::Guide(Guide::Ask { root, task, files }) => return status(mcp::ask_guide(&root, task, &files)),
        Command::Show { symbol, paths } => sources(&paths).map(|s| match index::show(&symbol, &s) {
            Ok(text) => {
                print!("{text}");
                true
            }
            Err(close) => {
                let hint =
                    if close.is_empty() { String::new() } else { format!("; the closest are {}", close.join(", ")) };
                println!("nothing is called `{symbol}`{hint}");
                false
            }
        }),
    };
    status(result.map(|ok| u8::from(!ok)))
}

/// The exit status: the command's, or 2 when it could not run.
fn status(result: Result<u8, Failure>) -> ExitCode {
    match result {
        Ok(code) => ExitCode::from(code),
        Err(Failure(message)) => {
            eprintln!("lotml: {message}");
            ExitCode::from(2)
        }
    }
}

fn sources(paths: &[PathBuf]) -> Result<Vec<index::Source>, Failure> {
    files::expand(paths)?
        .into_iter()
        .map(|path| Ok(index::Source { name: path.display().to_string(), text: files::read(&path)? }))
        .collect()
}

/// Format each file in place, or with `check` only say which are not canonical. A file with
/// syntax errors is left alone and reported.
fn fmt(paths: &[PathBuf], check: bool) -> Result<bool, Failure> {
    let mut ok = true;
    for path in files::expand(paths)? {
        let text = files::read(&path)?;
        match lotml_fmt::format(&text) {
            Ok(formatted) if formatted == text => {}
            Ok(_) if check => {
                println!("{}: not formatted", path.display());
                ok = false;
            }
            Ok(formatted) => {
                std::fs::write(&path, formatted)
                    .map_err(|e| Failure(format!("cannot write {}: {e}", path.display())))?;
                println!("{}: formatted", path.display());
            }
            Err(errors) => {
                println!(
                    "{}: {} syntax error{}; `lotml check` shows them",
                    path.display(),
                    errors.len(),
                    if errors.len() == 1 { "" } else { "s" }
                );
                ok = false;
            }
        }
    }
    Ok(ok)
}

fn explain(code: &str) -> Result<bool, Failure> {
    println!("{}", explanation(code)?);
    Ok(true)
}

/// The page explaining an error code, written `E0204`, `e204` or `204`.
pub fn explanation(code: &str) -> Result<String, Failure> {
    let digits: String = code.chars().filter(char::is_ascii_digit).collect();
    let wanted = format!("E{digits:0>4}");
    match lotml_diag::codes::find(&wanted) {
        Some(found) => Ok(format!("{}: {}\n\n{}", found.code, found.title, found.explanation)),
        None => Err(Failure(format!("there is no error code `{code}`; codes look like `E0204`"))),
    }
}

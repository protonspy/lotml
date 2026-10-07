//! A small native build timed step by step (plans/build-and-check-speed.md 1.1, 1.2): one program
//! compiled by the LLVM backend, then at `-O0` and at `-O2` the runtime compiled on its own, the
//! program's IR compiled on its own, the two objects linked, and the whole build as `lotml build`
//! runs it, with an empty runtime cache and with the runtime's object in it; each `runs` times. A
//! JSON line naming the compiler and the program, then one per level and step with the seconds of
//! each run.
//!
//!     cargo run --release -p lotml-llvm --example build_time -- <program.lotml> [runs]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use lotml_llvm::cache::Cache;
use lotml_llvm::driver::{self, Build, Level};

fn main() {
    let mut args = std::env::args().skip(1);
    let source = PathBuf::from(args.next().expect("the program to build"));
    let runs: usize = args.next().map_or(5, |r| r.parse().expect("a number of runs"));
    let clang = driver::find().unwrap_or_else(|e| panic!("{e}"));
    let name = source.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
    println!("{{\"compiler\": \"clang {}\", \"program\": {name:?}}}", clang.version);
    let text = std::fs::read_to_string(&source).expect("the program");
    let ll = lotml_llvm::compile(&text, &source).unwrap_or_else(|d| panic!("{:?}", d.first().map(|d| &d.message)));
    let scratch = scratch();
    std::fs::write(scratch.join("program.ll"), ll).expect("the IR");
    lotml_runtime::write(&scratch).expect("the runtime");
    for level in [Level::Debug, Level::Release] {
        let flags: &[&str] = if level == Level::Debug { &["-O0", "-g"] } else { &["-O2"] };
        let shown = if level == Level::Debug { "-O0" } else { "-O2" };
        let runtime = scratch.join("lotml.o");
        let program = scratch.join("program.o");
        let exe = exe(&scratch, "program");
        let step = |step: &str, args: &[&Path]| {
            let seconds: Vec<f64> = (0..runs)
                .map(|_| {
                    let mut command = Command::new(&clang.program);
                    command.args(flags).arg("-w").args(args);
                    timed(&mut command)
                })
                .collect();
            println!("{{\"level\": {shown:?}, \"step\": {step:?}, \"seconds\": {seconds:?}}}");
        };
        let include = scratch.as_path();
        step(
            "runtime",
            &[Path::new("-c"), &scratch.join("lotml.c"), Path::new("-I"), include, Path::new("-o"), &runtime],
        );
        step("program", &[Path::new("-c"), &scratch.join("program.ll"), Path::new("-o"), &program]);
        let libraries: &[&Path] = if cfg!(windows) { &[] } else { &[Path::new("-lm"), Path::new("-pthread")] };
        let link: Vec<&Path> =
            [&program, &runtime, Path::new("-o"), &exe].into_iter().chain(libraries.iter().copied()).collect();
        step("link", &link);
        let build = Build { level, counting: false, shared: false };
        let whole = |cache: &dyn Fn(usize) -> Cache| -> Vec<f64> {
            (0..runs)
                .map(|k| {
                    let cache = cache(k);
                    let started = Instant::now();
                    clang
                        .link_with(&scratch.join("program.ll"), &scratch, &exe, &[], build, Some(&cache))
                        .unwrap_or_else(|e| panic!("{e}"));
                    started.elapsed().as_secs_f64()
                })
                .collect()
        };
        let empty = |k: usize| Cache::at(&scratch.join(format!("cold-{shown}-{k}"))).expect("a cache");
        println!("{{\"level\": {shown:?}, \"step\": \"cold\", \"seconds\": {:?}}}", whole(&empty));
        let filled = Cache::at(&scratch.join(format!("warm-{shown}"))).expect("a cache");
        clang.link_with(&scratch.join("program.ll"), &scratch, &exe, &[], build, Some(&filled)).expect("a build");
        println!("{{\"level\": {shown:?}, \"step\": \"warm\", \"seconds\": {:?}}}", whole(&|_| filled.clone()));
    }
    let _ = std::fs::remove_dir_all(&scratch);
}

/// The seconds `command` took, which must succeed.
fn timed(command: &mut Command) -> f64 {
    let started = Instant::now();
    let out = command.output().expect("clang runs");
    let seconds = started.elapsed().as_secs_f64();
    assert!(out.status.success(), "{command:?}: {}", String::from_utf8_lossy(&out.stderr));
    seconds
}

/// A directory of its own for the builds, created anew: never one already there.
fn scratch() -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("lotml-build-time-{}-{nanos}", std::process::id()));
    let builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut owned = builder;
        owned.mode(0o700);
        owned
    };
    builder.create(&dir).expect("a new scratch directory");
    dir
}

fn exe(dir: &Path, stem: &str) -> PathBuf {
    dir.join(if cfg!(windows) { format!("{stem}.exe") } else { stem.to_string() })
}

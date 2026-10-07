//! The benchmarks of plans/lotml-roadmap.md task 4.3: each `<name>.lotml` of a directory built
//! by the C backend, and the `<name>.c` beside it written by hand, both by the C compiler the
//! driver finds with the same options; each run `runs` times. A JSON line naming the compiler,
//! then one per benchmark: its name, the seconds of each run of either program, and what each
//! printed.
//!
//!     cargo run --release -p lotml-c --example bench -- <directory> [runs]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("the directory of the benchmarks"));
    let runs: usize = args.next().map_or(5, |r| r.parse().expect("a number of runs"));
    let compiler = lotml_c::driver::find().unwrap_or_else(|e| panic!("{e}"));
    let program = compiler.program.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
    println!("{{\"compiler\": {program:?}}}");
    let scratch = scratch();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("the benchmarks")
        .filter_map(|e| e.ok()?.path().file_name()?.to_str()?.strip_suffix(".lotml").map(String::from))
        .filter(|name| dir.join(format!("{name}.c")).is_file())
        .collect();
    names.sort();
    for name in names {
        let out = scratch.join(&name);
        std::fs::create_dir_all(&out).expect("a scratch directory");
        let source = dir.join(format!("{name}.lotml"));
        let text = std::fs::read_to_string(&source).expect("the program");
        let c = lotml_c::compile(&text, &source)
            .unwrap_or_else(|d| panic!("{name}: {:?}", d.iter().map(|d| &d.message).collect::<Vec<_>>()));
        std::fs::write(out.join("program.c"), c).expect("the C");
        lotml_runtime::write(&out).expect("the runtime");
        let lotml = exe(&out, "program");
        compiler.build(&out.join("program.c"), &lotml, &[]).unwrap_or_else(|e| panic!("{e}"));
        let baseline = exe(&out, "baseline");
        let written = out.join("baseline.c");
        std::fs::copy(dir.join(format!("{name}.c")), &written).expect("the baseline");
        compiler.build(&written, &baseline, &[]).unwrap_or_else(|e| panic!("{e}"));
        let (c_times, c_out) = time(&baseline, runs);
        let (lotml_times, lotml_out) = time(&lotml, runs);
        println!(
            "{{\"name\": {:?}, \"c\": {c_times:?}, \"lotml\": {lotml_times:?}, \"c_out\": {:?}, \"lotml_out\": {:?}}}",
            name,
            c_out.trim(),
            lotml_out.trim()
        );
    }
    let _ = std::fs::remove_dir_all(&scratch);
}

/// A directory of its own for the builds, created anew: never one already there.
fn scratch() -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("lotml-bench-{}-{nanos}", std::process::id()));
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

/// The seconds of each of `runs` runs of `exe`, and what the first printed.
fn time(exe: &Path, runs: usize) -> (Vec<f64>, String) {
    let mut times = Vec::new();
    let mut printed = String::new();
    for k in 0..runs {
        let started = Instant::now();
        let out = Command::new(exe).output().expect("the benchmark runs");
        times.push(started.elapsed().as_secs_f64());
        assert!(out.status.success(), "{}: {}", exe.display(), String::from_utf8_lossy(&out.stderr));
        if k == 0 {
            printed = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        }
    }
    (times, printed)
}

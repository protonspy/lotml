//! Any text through the parser, the checker and the lowering of both targets, as `lotml run` and
//! `lotml build` take it: no input may panic (plans/frontend-robustness.md 3.1).

#![no_main]

use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    let worker = worker();
    worker.inputs.lock().expect("the worker's inputs").send(text.to_owned()).expect("the worker is running");
    worker.done.lock().expect("the worker's answers").recv().expect("the worker finished the input");
});

/// The thread the inputs are compiled on, with the stack the `lotml` binary gives the compiler, so
/// only what would overflow that is found. A panic there aborts the process, which libFuzzer
/// reports with the input.
struct Worker {
    inputs: Mutex<Sender<String>>,
    done: Mutex<Receiver<()>>,
}

fn worker() -> &'static Worker {
    static WORKER: OnceLock<Worker> = OnceLock::new();
    WORKER.get_or_init(|| {
        let (inputs, received) = channel::<String>();
        let (finished, done) = channel();
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(move || {
                for text in received {
                    compile(&text);
                    if finished.send(()).is_err() {
                        return;
                    }
                }
            })
            .expect("a worker thread");
        Worker { inputs: Mutex::new(inputs), done: Mutex::new(done) }
    })
}

fn compile(text: &str) {
    let path = Path::new("fuzz.lot");
    let _ = lotml_check::check_source(text);
    let _ = lotml_check::check_prefix(text);
    let _ = lotml_py::compile(text, path);
    for (tests, lines) in [(false, false), (true, true)] {
        let _ = lotml_llvm::compile_program(text, path, &lotml_check::Interfaces::new(), tests, lines);
    }
}

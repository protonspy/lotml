//! The fuzzer's first inputs: each program of the corpus (`harness/results/corpus/corpus.jsonl`)
//! written to `corpus/frontend/`, the directory `cargo fuzz run frontend` starts from.

fn main() {
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let corpus = here.join("../../harness/results/corpus/corpus.jsonl");
    let out = here.join("corpus").join("frontend");
    std::fs::create_dir_all(&out).expect("the corpus directory");
    let text = std::fs::read_to_string(&corpus).expect("the corpus");
    let mut written = 0;
    for (k, line) in text.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        let row: serde_json::Value = serde_json::from_str(line).expect("a JSON row");
        let Some(program) = row["lotml"].as_str() else { continue };
        std::fs::write(out.join(format!("{k:04}.lot")), program).expect("a seed");
        written += 1;
    }
    println!("{written} seeds in {}", out.display());
}

//! The interfaces of the binding coverage corpus's standard-library modules, bound from the typeshed
//! lotml carries, held to the files in `tests/golden/` (specs/rust-binder R1.2): the Python binder
//! wrote them, and moving typeshed's pin shows how they change. `LOTML_BLESS=1` rewrites them.

use std::path::Path;

use lotml_bind::typeshed::{Found, find};

#[test]
fn the_corpus_s_standard_library_binds_as_its_goldens_say() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let corpus: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(manifest.join("../../../harness/binding-corpus.json")).expect("the corpus"),
    )
    .expect("the corpus's JSON");
    let golden = manifest.join("tests").join("golden");
    let bless = std::env::var_os("LOTML_BLESS").is_some();
    let mut differ = Vec::new();
    for module in corpus["stdlib"]["modules"].as_array().expect("the standard-library modules") {
        let module = module.as_str().expect("a module name");
        let Found::Stub { text, .. } = find(module) else { panic!("typeshed has no stub for `{module}`") };
        let bound = lotml_bind::binder::interface(module, text, "typeshed").expect("the stub binds");
        let file = golden.join(format!("py.{module}.lotmli"));
        if bless {
            std::fs::create_dir_all(&golden).expect("the golden directory");
            std::fs::write(&file, &bound).expect("a golden");
        } else if std::fs::read_to_string(&file).ok().as_deref() != Some(bound.as_str()) {
            differ.push(module);
        }
    }
    assert!(differ.is_empty(), "these interfaces changed; review and rerun with LOTML_BLESS=1: {differ:?}");
}

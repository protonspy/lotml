//! The module being written: its constants — text, string literals, the places a program can
//! stop — and the declarations of what it calls, gathered while its functions are written.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

pub(crate) struct Module {
    /// The global holding the `.lot` file's name, as a panic names it.
    pub file: String,
    /// Each run of bytes the IR holds, by its global's name.
    texts: BTreeMap<Vec<u8>, String>,
    /// Each string literal as a runtime `lt_str` cell, by its text.
    literals: BTreeMap<String, String>,
    /// Each place a program can stop, by line and function: its global's name.
    sites: BTreeMap<(u32, String), String>,
    /// The runtime functions and intrinsics called, as their declarations.
    declared: BTreeSet<String>,
    /// Definitions of the module's own: types, descriptors, glue, closures, tables.
    pub definitions: String,
    /// The debug metadata, when the module carries line tables: each node's text, `!0` first.
    debug: Option<Vec<String>>,
    /// Each location a line table names, by line, column and function: its node.
    locations: BTreeMap<(u32, u32, usize), usize>,
}

impl Module {
    pub fn new(file: &str) -> Module {
        let mut module = Module {
            file: String::new(),
            texts: BTreeMap::new(),
            literals: BTreeMap::new(),
            sites: BTreeMap::new(),
            declared: BTreeSet::new(),
            definitions: String::new(),
            debug: None,
            locations: BTreeMap::new(),
        };
        module.file = module.text_z(file);
        module
    }

    /// The module, carrying line tables that name `path` (specs/llvm-parity R2.2): DWARF, and on
    /// Windows CodeView beside it, which its debuggers read.
    pub fn with_lines(file: &str) -> Module {
        let mut module = Module::new(file);
        let path = std::path::Path::new(file);
        let name = path.file_name().map_or(file.into(), |n| n.to_string_lossy().into_owned());
        let directory = path.parent().map_or(String::new(), |d| d.to_string_lossy().into_owned());
        module.debug = Some(vec![
            "distinct !DICompileUnit(language: DW_LANG_C99, file: !1, producer: \"lotml\", isOptimized: false, \
             runtimeVersion: 0, emissionKind: LineTablesOnly)"
                .into(),
            format!(
                "!DIFile(filename: \"{}\", directory: \"{}\")",
                ir_bytes(name.as_bytes()),
                ir_bytes(directory.as_bytes())
            ),
            "!DISubroutineType(types: !{})".into(),
        ]);
        module
    }

    fn node(&mut self, text: String) -> Option<usize> {
        let debug = self.debug.as_mut()?;
        debug.push(text);
        Some(debug.len() - 1)
    }

    /// The `DISubprogram` of the function `name`, declared at `line`; `None` without line tables.
    pub fn subprogram(&mut self, name: &str, line: u32) -> Option<usize> {
        self.node(format!(
            "distinct !DISubprogram(name: \"{}\", scope: !1, file: !1, line: {line}, type: !2, scopeLine: {line}, \
             spFlags: DISPFlagDefinition, unit: !0)",
            ir_bytes(name.as_bytes())
        ))
    }

    /// The `DILocation` of `line` and `column` in the function whose subprogram is `scope`.
    pub fn location(&mut self, line: u32, column: u32, scope: usize) -> usize {
        if let Some(&node) = self.locations.get(&(line, column, scope)) {
            return node;
        }
        let node =
            self.node(format!("!DILocation(line: {line}, column: {column}, scope: !{scope})")).expect("line tables");
        self.locations.insert((line, column, scope), node);
        node
    }

    /// The metadata at the module's end: its nodes and the flags naming their format, if any.
    pub fn metadata(&self) -> String {
        let Some(debug) = &self.debug else { return String::new() };
        let mut out = String::from("\n!llvm.dbg.cu = !{!0}\n");
        let n = debug.len();
        let mut flags = vec![format!("!{n}"), format!("!{}", n + 1)];
        if cfg!(windows) {
            flags.push(format!("!{}", n + 2));
        }
        let _ = writeln!(out, "!llvm.module.flags = !{{{}}}", flags.join(", "));
        for (k, text) in debug.iter().enumerate() {
            let _ = writeln!(out, "!{k} = {text}");
        }
        let _ = writeln!(out, "!{n} = !{{i32 7, !\"Dwarf Version\", i32 4}}");
        let _ = writeln!(out, "!{} = !{{i32 2, !\"Debug Info Version\", i32 3}}", n + 1);
        if cfg!(windows) {
            let _ = writeln!(out, "!{} = !{{i32 2, !\"CodeView\", i32 1}}", n + 2);
        }
        out
    }

    /// The global holding `bytes`, as they are.
    pub fn text(&mut self, bytes: &[u8]) -> String {
        let next = format!("@lt_text.{}", self.texts.len());
        self.texts.entry(bytes.to_vec()).or_insert(next).clone()
    }

    /// The global holding `text` and a NUL after it, as C reads a string.
    pub fn text_z(&mut self, text: &str) -> String {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(0);
        self.text(&bytes)
    }

    /// The global `lt_str` cell of the literal `text`.
    pub fn literal(&mut self, text: &str) -> String {
        let next = format!("@lt_lit.{}", self.literals.len());
        self.literals.entry(text.to_string()).or_insert(next).clone()
    }

    /// The global `lt_at` of `line` in the function whose LotML name is `function`.
    pub fn site(&mut self, line: u32, function: &str) -> String {
        let next = format!("@lt_site.{}", self.sites.len());
        self.sites.entry((line, function.to_string())).or_insert(next).clone()
    }

    pub fn declare(&mut self, declaration: &str) {
        self.declared.insert(declaration.to_string());
    }

    /// Declare the runtime function `name` as `lotml.h` declares it, and give its signature.
    pub fn runtime(&mut self, name: &str) -> &'static lotml_runtime::abi::Signature {
        let sig = lotml_runtime::abi::signature(name).unwrap_or_else(|| panic!("lotml.h declares no {name}"));
        self.declare(&sig.declaration(name));
        sig
    }

    /// The module's head: its types, constants and declarations, then its own definitions.
    pub fn header(&mut self) -> String {
        let mut fn_names = BTreeMap::new();
        let functions: Vec<String> = self.sites.keys().map(|(_, f)| f.clone()).collect();
        for function in functions {
            if let std::collections::btree_map::Entry::Vacant(slot) = fn_names.entry(function) {
                let global = self.text_z(slot.key());
                slot.insert(global);
            }
        }
        let mut out = String::from("; The LLVM IR of a LotML program, written by `lotml build --target llvm`.\n\n");
        out.push_str("%lt_at = type { ptr, i32, ptr }\n%lt_buf = type { ptr, i64, i64 }\n");
        out.push_str("%lt_cell = type { i32, i32 }\n");
        out.push_str("%lt_type = type { i64, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr, ptr }\n");
        out.push_str("%lt_list = type { %lt_cell, i64, i64, ptr, ptr }\n");
        out.push_str("%lt_str = type { %lt_cell, i64, i64, i64 }\n");
        out.push_str("%lt_closure = type { %lt_cell, ptr, ptr, ptr }\n\n");
        for (bytes, name) in &self.texts {
            let _ =
                writeln!(out, "{name} = private unnamed_addr constant [{} x i8] c\"{}\"", bytes.len(), ir_bytes(bytes));
        }
        for (text, name) in &self.literals {
            let size = text.len();
            let _ = writeln!(
                out,
                "{name} = private global {{ %lt_cell, i64, i64, i64, [{} x i8] }} {{ %lt_cell zeroinitializer, i64 {size}, i64 {}, i64 0, [{} x i8] c\"{}\\00\" }}, align 8",
                size + 1,
                text.chars().count(),
                size + 1,
                ir_bytes(text.as_bytes())
            );
        }
        for ((line, function), name) in &self.sites {
            let _ = writeln!(
                out,
                "{name} = private unnamed_addr constant %lt_at {{ ptr {}, i32 {line}, ptr {} }}",
                self.file, fn_names[function]
            );
        }
        out.push('\n');
        for d in &self.declared {
            out.push_str(d);
            out.push('\n');
        }
        out.push('\n');
        out.push_str(&self.definitions);
        out
    }
}

/// Bytes as the body of an LLVM `c"…"` constant: printable ASCII as is, every other byte as `\XX`.
pub fn ir_bytes(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &b in bytes {
        if (0x20..=0x7e).contains(&b) && b != b'"' && b != b'\\' {
            out.push(b as char);
        } else {
            let _ = write!(out, "\\{b:02X}");
        }
    }
    out
}

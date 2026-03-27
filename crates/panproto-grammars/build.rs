#![allow(
    missing_docs,
    clippy::uninlined_format_args,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::format_push_string,
    clippy::single_char_add_str
)]

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
struct GrammarSpec {
    extensions: Vec<String>,
    #[serde(default)]
    c_symbol: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    directory: Option<String>,
    #[allow(dead_code)]
    repo: String,
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let grammars_toml_path = workspace_root.join("grammars.toml");
    let grammars_dir = workspace_root.join("grammars");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    println!(
        "cargo:rerun-if-changed={}",
        grammars_toml_path.display()
    );

    let toml_str = fs::read_to_string(&grammars_toml_path)
        .expect("failed to read grammars.toml");
    let manifest: BTreeMap<String, GrammarSpec> =
        toml::from_str(&toml_str).expect("failed to parse grammars.toml");

    let mut enabled: Vec<(String, GrammarSpec)> = Vec::new();

    for (name, spec) in manifest {
        let feature_env = format!(
            "CARGO_FEATURE_LANG_{}",
            name.to_uppercase().replace('-', "_")
        );
        if env::var(&feature_env).is_ok() {
            enabled.push((name, spec));
        }
    }

    // Compile C sources for each enabled grammar. Track which ones succeed.
    let mut compiled_flags: Vec<bool> = Vec::new();
    for (name, spec) in &enabled {
        let lang_dir = grammars_dir.join(name).join("src");
        if !lang_dir.exists() {
            println!(
                "cargo:warning=Grammar '{}' sources not found at {}; run tools/fetch-grammars.py",
                name,
                lang_dir.display()
            );
            compiled_flags.push(false);
            continue;
        }

        compiled_flags.push(compile_grammar(name, spec, &lang_dir));
    }

    // Only include successfully compiled grammars in the bindings.
    let compiled: Vec<&(String, GrammarSpec)> = enabled
        .iter()
        .zip(compiled_flags.iter())
        .filter(|(_, ok)| **ok)
        .map(|(entry, _)| entry)
        .collect();

    // Generate the Rust binding file.
    let generated = generate_rust_bindings(&compiled, &grammars_dir);
    let out_file = out_dir.join("grammar_table.rs");
    fs::write(&out_file, generated).expect("failed to write grammar_table.rs");
}

fn compile_grammar(name: &str, spec: &GrammarSpec, src_dir: &Path) -> bool {
    let c_symbol = spec
        .c_symbol
        .as_deref()
        .unwrap_or(name);
    let parser_c = src_dir.join("parser.c");
    let scanner_c = src_dir.join("scanner.c");
    let scanner_cc = src_dir.join("scanner.cc");

    if !parser_c.exists() {
        println!("cargo:warning=Grammar '{name}': no parser.c found, skipping");
        return false;
    }

    println!("cargo:rerun-if-changed={}", parser_c.display());

    // Compile parser.c (and scanner.c if present).
    let mut build = cc::Build::new();
    build
        .file(&parser_c)
        .include(src_dir)
        .std("c11")
        .warnings(false)
        .cargo_warnings(false);

    // Add common/ directory as include path (for grammars like PHP).
    let common_dir = src_dir.join("common");
    if common_dir.is_dir() {
        build.include(&common_dir);
    }

    // Scan scanner source for #include directives that reference files not
    // present in src_dir. If found, search sibling grammar src/ directories
    // for the missing header. This handles grammars that extend others (e.g.,
    // Angular/Svelte including tag.h from the HTML grammar).
    let grammars_parent = src_dir.parent().and_then(Path::parent);
    if let Some(all_grammars) = grammars_parent {
        let extra_includes = find_missing_includes(src_dir, all_grammars);
        for inc_dir in &extra_includes {
            build.include(inc_dir);
        }
    }

    if scanner_c.exists() {
        build.file(&scanner_c);
        println!("cargo:rerun-if-changed={}", scanner_c.display());
    }

    // Use try_compile to gracefully handle compilation failures.
    if let Err(e) = build.try_compile(&format!("tree_sitter_{c_symbol}")) {
        println!("cargo:warning=Grammar '{name}' failed to compile: {e}");
        return false;
    }

    // Compile C++ scanner separately if present.
    if scanner_cc.exists() {
        println!("cargo:rerun-if-changed={}", scanner_cc.display());

        let mut cpp_build = cc::Build::new();
        cpp_build
            .cpp(true)
            .file(&scanner_cc)
            .include(src_dir)
            .std("c++14")
            .warnings(false)
            .cargo_warnings(false);

        // Add sibling grammar includes for C++ scanners too.
        if let Some(all_grammars) = grammars_parent {
            for inc_dir in &find_missing_includes(src_dir, all_grammars) {
                cpp_build.include(inc_dir);
            }
        }

        if let Err(e) = cpp_build
            .try_compile(&format!("tree_sitter_{c_symbol}_scanner"))
        {
            println!("cargo:warning=Grammar '{name}' C++ scanner failed: {e}");
            return false;
        }
    }

    true
}

fn generate_rust_bindings(
    enabled: &[&(String, GrammarSpec)],
    grammars_dir: &Path,
) -> String {
    let mut code = String::new();

    code.push_str("// Auto-generated by build.rs. Do not edit.\n\n");

    // Extern declarations and node-types includes.
    code.push_str("unsafe extern \"C\" {\n");
    let mut available: Vec<(&String, &GrammarSpec)> = Vec::new();
    for (name, spec) in enabled {
        let c_symbol = spec.c_symbol.as_deref().unwrap_or(name);
        let lang_dir = grammars_dir.join(name).join("src");

        if !lang_dir.join("parser.c").exists() {
            continue;
        }

        code.push_str(&format!(
            "    fn tree_sitter_{c_symbol}() -> *const ();\n"
        ));
        available.push((name, spec));
    }
    code.push_str("}\n\n");

    for (name, _spec) in &available {
        let lang_dir = grammars_dir.join(name).join("src");
        let node_types_path = lang_dir.join("node-types.json");
        let abs_path = node_types_path
            .canonicalize()
            .unwrap_or_else(|_| node_types_path.clone());
        let const_name = name.to_uppercase().replace('-', "_");
        code.push_str(&format!(
            "const {const_name}_NODE_TYPES: &[u8] = include_bytes!(\"{}\");\n",
            abs_path.display(),
        ));
    }

    code.push_str("\n");

    // GrammarEntry struct using a raw pointer to the language function.
    code.push_str("pub(crate) struct GrammarEntry {\n");
    code.push_str("    pub name: &'static str,\n");
    code.push_str("    pub extensions: &'static [&'static str],\n");
    code.push_str("    pub language_fn_ptr: *const (),\n");
    code.push_str("    pub node_types: &'static [u8],\n");
    code.push_str("}\n\n");
    code.push_str("unsafe impl Send for GrammarEntry {}\n");
    code.push_str("unsafe impl Sync for GrammarEntry {}\n");

    code.push_str("\npub(crate) fn enabled_grammars() -> Vec<GrammarEntry> {\n");
    code.push_str("    let mut grammars = Vec::new();\n");

    for (name, spec) in &available {
        let c_symbol = spec.c_symbol.as_deref().unwrap_or(name);

        let exts: Vec<String> = spec
            .extensions
            .iter()
            .map(|e| format!("\"{}\"", e))
            .collect();
        let exts_str = exts.join(", ");
        let const_name = name.to_uppercase().replace('-', "_");

        code.push_str(&format!(
            "    grammars.push(GrammarEntry {{\n\
             \x20       name: \"{name}\",\n\
             \x20       extensions: &[{exts_str}],\n\
             \x20       language_fn_ptr: tree_sitter_{c_symbol} as *const (),\n\
             \x20       node_types: {const_name}_NODE_TYPES,\n\
             \x20   }});\n"
        ));
    }

    code.push_str("    grammars\n}\n");

    // Extension-to-language lookup.
    code.push_str(
        "\npub(crate) fn ext_to_lang(ext: &str) -> Option<&'static str> {\n\
         \x20   match ext {\n",
    );

    // Build extension map (first-registered wins for conflicts).
    let mut ext_map: BTreeMap<String, String> = BTreeMap::new();
    for (name, spec) in enabled {
        let lang_dir = grammars_dir.join(name).join("src");
        if !lang_dir.join("parser.c").exists() {
            continue;
        }
        for ext in &spec.extensions {
            ext_map.entry(ext.clone()).or_insert_with(|| name.clone());
        }
    }

    for (ext, lang) in &ext_map {
        code.push_str(&format!(
            "        \"{}\" => Some(\"{}\"),\n",
            ext, lang
        ));
    }

    code.push_str("        _ => None,\n    }\n}\n");

    code
}

/// Scan C/C++ source files in `src_dir` for `#include "..."` directives that
/// reference files not present locally. For each missing header, search all
/// sibling grammar `src/` directories under `all_grammars` and return the
/// directories where the header was found.
///
/// This handles grammars that extend other grammars (e.g., Angular including
/// `tag.h` from the HTML grammar, or Svelte including from HTML).
fn find_missing_includes(src_dir: &Path, all_grammars: &Path) -> Vec<PathBuf> {
    let mut needed_dirs: BTreeSet<PathBuf> = BTreeSet::new();

    // Collect #include "..." from scanner.c, scanner.cc, and parser.c.
    for filename in &["scanner.c", "scanner.cc", "parser.c"] {
        let file = src_dir.join(filename);
        if !file.exists() {
            continue;
        }
        let content = match fs::read_to_string(&file) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("#include") {
                continue;
            }
            // Match #include "header.h" (not <header.h>).
            if let Some(start) = trimmed.find('"') {
                if let Some(end) = trimmed[start + 1..].find('"') {
                    let header = &trimmed[start + 1..start + 1 + end];
                    // Skip standard tree_sitter headers and local files.
                    if header.starts_with("tree_sitter/") {
                        continue;
                    }
                    // Check if the header exists locally.
                    if src_dir.join(header).exists() {
                        continue;
                    }
                    // Search sibling grammars for this header.
                    let header_basename = Path::new(header)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    if let Ok(entries) = fs::read_dir(all_grammars) {
                        for entry in entries.flatten() {
                            let sibling_src = entry.path().join("src");
                            if sibling_src == *src_dir || !sibling_src.is_dir() {
                                continue;
                            }
                            if sibling_src.join(&header_basename).exists() {
                                needed_dirs.insert(sibling_src);
                            }
                        }
                    }
                }
            }
        }
    }

    needed_dirs.into_iter().collect()
}

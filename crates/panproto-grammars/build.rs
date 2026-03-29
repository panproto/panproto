#![allow(
    missing_docs,
    clippy::uninlined_format_args,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::format_push_string,
    clippy::single_char_add_str
)]

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

    println!("cargo:rerun-if-changed={}", grammars_toml_path.display());

    // Outside the workspace (e.g., when installed from crates.io), grammars.toml
    // and grammars/ are not available. Generate an empty grammar table; users add
    // languages via panproto_parse::ParserRegistry::register() with individual
    // grammar crates (tree-sitter-python, tree-sitter-rust, etc.).
    let Ok(toml_str) = fs::read_to_string(&grammars_toml_path) else {
        let stub = "\
            pub(crate) struct GrammarEntry {\n\
            \x20   pub name: &'static str,\n\
            \x20   pub extensions: &'static [&'static str],\n\
            \x20   pub language_fn_ptr: *const (),\n\
            \x20   pub node_types: &'static [u8],\n\
            }\n\
            unsafe impl Send for GrammarEntry {}\n\
            unsafe impl Sync for GrammarEntry {}\n\
            pub(crate) fn enabled_grammars() -> Vec<GrammarEntry> { Vec::new() }\n\
            pub(crate) fn ext_to_lang(_: &str) -> Option<&'static str> { None }\n";
        fs::write(out_dir.join("grammar_table.rs"), stub)
            .expect("failed to write grammar_table.rs");
        return;
    };
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

    // Post-process each grammar's static library to localize internal symbols,
    // preventing duplicate-symbol linker errors across grammars.
    localize_internal_symbols(&enabled, &compiled_flags);

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
    let c_symbol = spec.c_symbol.as_deref().unwrap_or(name);
    let parser_c = src_dir.join("parser.c");
    let scanner_cc = src_dir.join("scanner.cc");

    if !parser_c.exists() {
        println!("cargo:warning=Grammar '{name}': no parser.c found, skipping");
        return false;
    }

    // Compile all .c files in src/ (parser.c, scanner.c, and any auxiliary
    // files like yaml's schema.core.c).
    let mut build = cc::Build::new();
    build
        .include(src_dir)
        .std("c11")
        .warnings(false)
        .cargo_warnings(false);

    for entry in fs::read_dir(src_dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "c") {
            build.file(&path);
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }

    // Add common/ directory as include path (for grammars like PHP).
    let common_dir = src_dir.join("common");
    if common_dir.is_dir() {
        build.include(&common_dir);
    }

    // Use try_compile to gracefully handle compilation failures.
    let lib_name = format!("tree_sitter_{c_symbol}");
    if let Err(e) = build.try_compile(&lib_name) {
        println!("cargo:warning=Grammar '{name}' failed to compile: {e}");
        return false;
    }

    // Compile C++ scanner separately if present. Each scanner is wrapped in a
    // unique named namespace to prevent COMDAT symbol collisions when rust-lld
    // deduplicates inline methods (e.g., Scanner::Scanner()) across grammars.
    let scanner_lib_name = format!("tree_sitter_{c_symbol}_scanner");
    if scanner_cc.exists() {
        println!("cargo:rerun-if-changed={}", scanner_cc.display());

        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let ns_name = format!("panproto_grammar_{}", name.replace('-', "_"));
        let abs_scanner = scanner_cc
            .canonicalize()
            .unwrap_or_else(|_| scanner_cc.clone());

        // Write a wrapper that includes the original scanner.cc inside a unique
        // namespace. Pre-include tree_sitter/parser.h at global scope so that
        // TSLexer/TSLanguage types are declared globally (include guards prevent
        // re-declaration when scanner.cc includes it again inside the namespace).
        // extern "C" functions inside a named namespace retain external C linkage
        // and unmangled names, so tree_sitter_*_external_scanner_* symbols remain
        // visible to the linker.
        let wrapper_path = out_dir.join(format!("{name}_scanner_wrapper.cc"));
        let wrapper = format!(
            "#include \"tree_sitter/parser.h\"\n\
             namespace {ns_name} {{\n\
             #include \"{scanner}\"\n\
             }} // namespace {ns_name}\n",
            scanner = abs_scanner.display().to_string().replace('\\', "/"),
        );
        fs::write(&wrapper_path, &wrapper).expect("failed to write scanner wrapper");

        let mut cpp_build = cc::Build::new();
        cpp_build
            .cpp(true)
            .file(&wrapper_path)
            .include(src_dir)
            .std("c++14")
            .flag("-fno-exceptions")
            .flag("-fno-rtti")
            .warnings(false)
            .cargo_warnings(false);

        if let Err(e) = cpp_build.try_compile(&scanner_lib_name) {
            println!("cargo:warning=Grammar '{name}' C++ scanner failed: {e}");
            return false;
        }
    }

    true
}

/// After all grammars are compiled, localize non-`tree_sitter_*` symbols in
/// each static library to prevent duplicate-symbol linker errors. Hand-written
/// scanner.c files reuse common internal names (e.g., `scan_comment` appears
/// in 16+ grammars); localizing makes them file-scoped in the archive.
///
/// On macOS: uses `ld -r -exported_symbol` to create a relocatable object with
/// only `tree_sitter_*` symbols exported, then re-archives.
/// On Linux: uses `objcopy --keep-global-symbol` with wildcard matching.
fn localize_internal_symbols(enabled: &[(String, GrammarSpec)], compiled_flags: &[bool]) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    for (i, (name, spec)) in enabled.iter().enumerate() {
        if !compiled_flags[i] {
            continue;
        }
        let c_symbol = spec.c_symbol.as_deref().unwrap_or(name.as_str());

        let mut libs = vec![format!("tree_sitter_{c_symbol}")];
        let scanner_lib = format!("tree_sitter_{c_symbol}_scanner");
        if out_dir.join(format!("lib{scanner_lib}.a")).exists() {
            libs.push(scanner_lib);
        }

        for lib in &libs {
            let lib_path = out_dir.join(format!("lib{lib}.a"));
            if !lib_path.exists() {
                continue;
            }

            let ok = if target_os == "macos" || target_os.is_empty() {
                localize_macos(&lib_path, lib, &out_dir, &target_arch)
            } else {
                localize_linux(&lib_path)
            };

            if !ok {
                println!(
                    "cargo:warning=Failed to localize symbols in {}, \
                     duplicate-symbol errors may occur",
                    lib_path.display()
                );
            }
        }
    }
}

/// macOS: extract .o files from the archive, use `ld -r -exported_symbol` to
/// produce a single relocatable object with only `_tree_sitter_*` global, then
/// re-archive.
fn localize_macos(lib_path: &Path, lib_name: &str, out_dir: &Path, target_arch: &str) -> bool {
    let work_dir = out_dir.join(format!("{lib_name}_localize"));
    let _ = fs::remove_dir_all(&work_dir);
    if fs::create_dir_all(&work_dir).is_err() {
        return false;
    }

    // Extract .o files from the archive.
    let ar_status = Command::new("ar")
        .args(["x"])
        .arg(lib_path)
        .current_dir(&work_dir)
        .status();
    if !ar_status.is_ok_and(|s| s.success()) {
        return false;
    }

    // Collect extracted .o files.
    let objects: Vec<PathBuf> = fs::read_dir(&work_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "o"))
        .map(|e| e.path())
        .collect();

    if objects.is_empty() {
        return true;
    }

    let arch = match target_arch {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        other => other,
    };

    // Partial-link all .o files into one relocatable object, exporting only
    // tree_sitter_* symbols. All other globals become file-local.
    let merged = work_dir.join("merged.o");
    let mut ld_cmd = Command::new("xcrun");
    ld_cmd
        .args([
            "ld",
            "-r",
            "-arch",
            arch,
            "-exported_symbol",
            "_tree_sitter_*",
            "-o",
        ])
        .arg(&merged);
    for obj in &objects {
        ld_cmd.arg(obj);
    }

    let ld_status = ld_cmd.status();
    if !ld_status.is_ok_and(|s| s.success()) {
        return false;
    }

    // Replace the original archive with one containing only the merged object.
    let _ = fs::remove_file(lib_path);
    let ar_status = Command::new("ar")
        .args(["rcs"])
        .arg(lib_path)
        .arg(&merged)
        .status();

    let _ = fs::remove_dir_all(&work_dir);
    ar_status.is_ok_and(|s| s.success())
}

/// Linux: use objcopy (or llvm-objcopy) with wildcard to keep only
/// `tree_sitter_*` symbols global in each archive member.
fn localize_linux(lib_path: &Path) -> bool {
    // Try objcopy first (GNU binutils), then llvm-objcopy.
    for tool in &["objcopy", "llvm-objcopy"] {
        let status = Command::new(tool)
            .args(["--wildcard", "--keep-global-symbol=tree_sitter_*"])
            .arg(lib_path)
            .status();

        if status.is_ok_and(|s| s.success()) {
            return true;
        }
    }
    false
}

fn generate_rust_bindings(enabled: &[&(String, GrammarSpec)], grammars_dir: &Path) -> String {
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

        code.push_str(&format!("    fn tree_sitter_{c_symbol}() -> *const ();\n"));
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
        // Use forward slashes for include_bytes! paths (Windows backslashes are
        // interpreted as escape sequences in string literals).
        let path_str = abs_path.display().to_string().replace('\\', "/");
        code.push_str(&format!(
            "const {const_name}_NODE_TYPES: &[u8] = include_bytes!(\"{path_str}\");\n",
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
        code.push_str(&format!("        \"{}\" => Some(\"{}\"),\n", ext, lang));
    }

    code.push_str("        _ => None,\n    }\n}\n");

    code
}

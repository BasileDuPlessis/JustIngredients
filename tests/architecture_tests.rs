//! The engine knows nothing of the doors (`specs/tasks/engine-api.md`, §3): nothing under
//! `src/engine/` names Telegram, an HTTP or MCP crate, or a module of the crate outside an
//! allowlist (`crate::bot` and `crate::observability` both use Telegram).

use std::path::{Path, PathBuf};

/// The modules of the crate the engine may use.
const ALLOWED_MODULES: &[&str] = &[
    "conversion",
    "db",
    "engine",
    "errors",
    "extraction",
    "recipe_parts",
    "search",
    "text_processing",
    "unit_view",
    "units",
    "validation",
];

/// Crates of the doors.
const DOOR_CRATES: &[&str] = &[
    "axum",
    "hyper",
    "rmcp",
    "teloxide",
    "teloxide_tests",
    "tower",
];

fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            rust_files(&path, found)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
    Ok(())
}

/// The modules of the crate a source names (`crate::db::…`, `crate::{db, search}` is not used
/// in the engine).
fn crate_modules(source: &str) -> Vec<String> {
    source
        .match_indices("crate::")
        .map(|(at, _)| {
            source[at + "crate::".len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect()
        })
        .collect()
}

/// Whether a source names `name` as a crate: `name::` or `use name` at the start of a path.
fn names_crate(source: &str, name: &str) -> bool {
    source.match_indices(name).any(|(at, _)| {
        let before = source[..at].chars().next_back();
        let starts_path =
            before.is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'));
        let after = &source[at + name.len()..];
        starts_path
            && (after.starts_with("::") || after.starts_with(';') || after.starts_with(" as "))
    })
}

#[test]
fn the_engine_uses_only_its_allowlist() -> std::io::Result<()> {
    let engine = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/engine");
    let mut files = Vec::new();
    rust_files(&engine, &mut files)?;
    assert!(!files.is_empty(), "no engine sources found");
    for file in files {
        let source = std::fs::read_to_string(&file)?;
        for module in crate_modules(&source) {
            assert!(
                ALLOWED_MODULES.contains(&module.as_str()),
                "{} uses crate::{module}, outside the engine's allowlist",
                file.display()
            );
        }
        for door in DOOR_CRATES {
            assert!(
                !names_crate(&source, door),
                "{} names the crate {door}",
                file.display()
            );
        }
    }
    Ok(())
}

#[test]
fn the_check_sees_what_it_must_refuse() {
    assert_eq!(
        crate_modules("use crate::bot::cards;\nlet x = crate::db::f();"),
        ["bot", "db"]
    );
    assert!(names_crate("use teloxide::prelude::*;", "teloxide"));
    assert!(names_crate("fn f(bot: &teloxide::Bot) {}", "teloxide"));
    assert!(names_crate("use axum;", "axum"));
    assert!(!names_crate("// a teloxide_like word", "teloxide"));
    assert!(!names_crate("let towers = 3;", "tower"));
    assert!(
        !names_crate("crate::db::tower::x", "tower"),
        "a module path, not the crate"
    );
}

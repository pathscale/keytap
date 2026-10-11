//! Fixture runner for keytap-rules.
//!
//! Usage:
//!   cargo run -p fixtures -- check
//!   cargo run -p fixtures -- regen
//!
//! Fixture files live in fixtures/cases/*.txt. Each file is a transcript of
//! input lines (prefix `>`) and expected action lines (prefix `<`).
//! `check` compares byte-for-byte against the file regenerated from the engine.

use keytap_rules::{Action, Config, Event, run_sequence};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn cases_dir() -> PathBuf {
    // Prefer CARGO_MANIFEST_DIR/../../fixtures/cases when run via cargo.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let via_manifest = manifest.join("../../fixtures/cases");
    if via_manifest.is_dir() {
        return via_manifest.canonicalize().unwrap_or(via_manifest);
    }
    PathBuf::from("fixtures/cases")
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "check".to_string());
    match cmd.as_str() {
        "check" => cmd_check(),
        "regen" => cmd_regen(),
        "list" => cmd_list(),
        other => {
            eprintln!("unknown command `{other}`; use check, regen, or list");
            ExitCode::from(2)
        }
    }
}

fn cmd_list() -> ExitCode {
    let dir = cases_dir();
    let mut names = list_cases(&dir);
    names.sort();
    for n in names {
        println!("{n}");
    }
    ExitCode::SUCCESS
}

fn cmd_check() -> ExitCode {
    let dir = cases_dir();
    let mut names = list_cases(&dir);
    names.sort();
    if names.is_empty() {
        eprintln!("no fixtures in {}", dir.display());
        return ExitCode::from(1);
    }
    let mut failed = 0usize;
    let mut passed = 0usize;
    for name in &names {
        let path = dir.join(name);
        let original = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("read {}: {e}", path.display());
        });
        match regenerate(&original) {
            Ok(canonical) => {
                if canonical == original {
                    passed += 1;
                    println!("ok {name}");
                } else {
                    failed += 1;
                    eprintln!("FAIL {name}: output differs (run regen to update)");
                    diff_hint(&original, &canonical);
                }
            }
            Err(e) => {
                failed += 1;
                eprintln!("FAIL {name}: {e}");
            }
        }
    }
    println!(
        "{passed} passed, {failed} failed, {} total",
        passed + failed
    );
    if failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn cmd_regen() -> ExitCode {
    let dir = cases_dir();
    let mut names = list_cases(&dir);
    names.sort();
    for name in &names {
        let path = dir.join(name);
        let original = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("read {}: {e}", path.display());
        });
        match regenerate(&original) {
            Ok(canonical) => {
                fs::write(&path, &canonical).unwrap_or_else(|e| {
                    panic!("write {}: {e}", path.display());
                });
                println!("regen {name}");
            }
            Err(e) => {
                eprintln!("FAIL {name}: {e}");
                return ExitCode::from(1);
            }
        }
    }
    ExitCode::SUCCESS
}

fn list_cases(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for ent in entries.flatten() {
        let name = ent.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".txt") {
            out.push(name.into_owned());
        }
    }
    out
}

struct ParsedFixture {
    /// Exact comment / blank lines before first `>`, preserved on regen.
    header: String,
    inputs: Vec<Event>,
}

fn parse_fixture(text: &str) -> Result<ParsedFixture, String> {
    let mut header = String::new();
    let mut inputs = Vec::new();
    let mut saw_input = false;
    for (idx, raw) in text.lines().enumerate() {
        let line_no = idx + 1;
        if !saw_input {
            let trimmed = raw.trim_start();
            if trimmed.starts_with('>') {
                saw_input = true;
            } else if trimmed.starts_with('<') {
                return Err(format!("line {line_no}: action before any input"));
            } else {
                header.push_str(raw);
                header.push('\n');
                continue;
            }
        }
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            // Ignore blank/comment lines in the body; regen will not re-emit them.
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('>') {
            inputs
                .push(Event::parse_line(rest.trim()).map_err(|e| format!("line {line_no}: {e}"))?);
        } else if trimmed.starts_with('<') {
            // Expected actions are ignored on parse; regen recomputes them.
            continue;
        } else {
            return Err(format!("line {line_no}: expected `>` or `<`"));
        }
    }
    if inputs.is_empty() {
        return Err("no input events".into());
    }
    Ok(ParsedFixture { header, inputs })
}

fn regenerate(text: &str) -> Result<String, String> {
    let parsed = parse_fixture(text)?;
    let cfg = Config::default();
    let mut eng_actions_per_input: Vec<Vec<Action>> = Vec::new();
    let mut eng = keytap_rules::Engine::new(cfg.clone());
    for ev in &parsed.inputs {
        eng_actions_per_input.push(eng.handle(*ev));
    }
    // Also verify run_sequence flat equals concatenated.
    let flat: Vec<Action> = eng_actions_per_input.iter().flatten().cloned().collect();
    let flat2 = run_sequence(&cfg, &parsed.inputs);
    if flat != flat2 {
        return Err("run_sequence mismatch with per-event handle".into());
    }

    let mut out = String::new();
    out.push_str(&parsed.header);
    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    for (ev, actions) in parsed.inputs.iter().zip(eng_actions_per_input.iter()) {
        out.push('>');
        out.push(' ');
        out.push_str(&ev.render());
        out.push('\n');
        for a in actions {
            out.push('<');
            out.push(' ');
            out.push_str(&a.render());
            out.push('\n');
        }
    }
    Ok(out)
}

fn diff_hint(a: &str, b: &str) {
    let al: Vec<&str> = a.lines().collect();
    let bl: Vec<&str> = b.lines().collect();
    let n = al.len().max(bl.len());
    let mut stderr = io::stderr();
    let mut shown = 0usize;
    for i in 0..n {
        let l = al.get(i).copied().unwrap_or("<missing>");
        let r = bl.get(i).copied().unwrap_or("<missing>");
        if l != r {
            let _ = writeln!(stderr, "  line {}: got {l:?}", i + 1);
            let _ = writeln!(stderr, "         exp {r:?}");
            shown += 1;
            if shown >= 8 {
                let _ = writeln!(stderr, "  ...");
                break;
            }
        }
    }
}

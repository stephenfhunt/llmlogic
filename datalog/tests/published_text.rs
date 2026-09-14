//! What the skill ships and what the engine prints are read by an agent in someone
//! else's project, with none of this repository's history: `skill/SKILL.md`, its
//! `recipes/` and `examples/`, and every message the binary writes. Provenance —
//! spec sections, defect ids, dates, the codebases the engine was tried on, paths
//! into this repository — belongs in `spec.md` §17, `bugs/` and `notes/` (§17
//! 2026-09-14 (afternoon)).
//!
//! This catches the common leaks. Prose that only makes sense with the background
//! is a reading discipline (`AGENTS.md`), not something a pattern can see.

use std::fs;
use std::path::{Path, PathBuf};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// Source files compiled only under `cfg(test)`, so nothing in them is printed
/// by the binary.
const TEST_ONLY: &[&str] = &["src/testgen.rs", "src/engine/naive.rs"];

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn a_spec_section(text: &str) -> bool {
    text.contains('§')
}

fn a_defect_id(text: &str) -> bool {
    text.match_indices("bugs/").any(|(at, _)| {
        text[at + "bugs/".len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
    })
}

fn a_project_document(text: &str) -> bool {
    contains_any(
        text,
        &[
            "spec.md",
            "testing.md",
            "decisions.md",
            "ROADMAP",
            "worklog",
            "AGENTS.md",
            "EXPERIMENTS.md",
            "notes/",
        ],
    )
}

fn dogfooding(text: &str) -> bool {
    text.to_lowercase().contains("dogfood")
}

fn a_subject_nobody_introduced(text: &str) -> bool {
    contains_any(
        &text.to_lowercase(),
        &[
            "crate above",
            "project above",
            "codebase above",
            "run above",
        ],
    )
}

fn a_codebase_the_engine_was_tried_on(text: &str) -> bool {
    contains_any(
        &text.to_lowercase(),
        &["grafana", "sqlparse", "tsdl", "llmlogic"],
    )
}

fn the_experiments(text: &str) -> bool {
    text.to_lowercase().contains("experiment")
}

/// A `20YY-MM-DD` that is not a temporal literal: not after `@`, and not part of
/// a longer run of digits.
fn a_dated_event(text: &str) -> bool {
    let bytes = text.as_bytes();
    (0..bytes.len().saturating_sub(9)).any(|at| {
        let date = &bytes[at..at + 10];
        let shaped = date.iter().enumerate().all(|(i, b)| match i {
            4 | 7 => *b == b'-',
            _ => b.is_ascii_digit(),
        }) && date.starts_with(b"20");
        let before = at.checked_sub(1).map(|i| bytes[i]);
        let after = bytes.get(at + 10).copied();
        shaped
            && !matches!(before, Some(b'@' | b'-' | b'0'..=b'9'))
            && !after.is_some_and(|b| b.is_ascii_digit())
    })
}

fn a_link_out_of_the_bundle(text: &str) -> bool {
    text.contains("](../")
}

/// A leak's name, and the check that finds it in one line of text.
type LeakCheck = (&'static str, fn(&str) -> bool);

const LEAKS: &[LeakCheck] = &[
    ("a spec section", a_spec_section),
    ("a defect id", a_defect_id),
    ("a project document", a_project_document),
    ("dogfooding", dogfooding),
    ("a subject nobody introduced", a_subject_nobody_introduced),
    (
        "a codebase the engine was tried on",
        a_codebase_the_engine_was_tried_on,
    ),
    ("the experiments", the_experiments),
    ("a dated event", a_dated_event),
    ("a link out of the bundle", a_link_out_of_the_bundle),
];

#[test]
fn each_leak_check_catches_its_leak_and_passes_its_lookalike() {
    let leaks = [
        "arguments are flat (§4)",
        "(bugs/004)",
        "see spec.md",
        "found dogfooding",
        "in the crate above",
        "on `@grafana/ui`",
        "the experiments' answer key",
        "worked end to end (2026-07-27)",
        "[guide](../docs/agent-skill.md)",
    ];
    assert_eq!(leaks.len(), LEAKS.len());
    for ((what, check), leak) in LEAKS.iter().zip(leaks) {
        assert!(check(leak), "{what} misses {leak:?}");
    }
    let lookalikes = [
        "`@2026-08-19T10:30:00`",
        "p(@2026-08-19).",
        "a bugs/ directory",
        "the project's own function names",
        "section 4, below",
        "release notes",
    ];
    for lookalike in lookalikes {
        for (what, check) in LEAKS {
            assert!(!check(lookalike), "{what} flags {lookalike:?}");
        }
    }
}

fn files_under(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("directory reads") {
        let path = entry.expect("entry reads").path();
        if path.is_dir() {
            files_under(&path, extension, out);
        } else if path.extension().is_some_and(|e| e == extension) {
            out.push(path);
        }
    }
}

/// The lines of a source file the binary can print from: not comments or
/// attributes, and not inside a `#[cfg(test)]` module.
fn printable_lines(text: &str) -> Vec<(usize, &str)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut kept = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let trimmed = lines[index].trim();
        let opens_test_module = trimmed == "#[cfg(test)]"
            && lines.get(index + 1).is_some_and(|next| {
                let next = next.trim();
                next.contains("mod ") && next.ends_with('{')
            });
        if opens_test_module {
            let mut depth = 0i64;
            index += 1;
            while index < lines.len() {
                let code = lines[index].split("//").next().unwrap_or("");
                depth += code.matches('{').count() as i64 - code.matches('}').count() as i64;
                index += 1;
                if depth <= 0 {
                    break;
                }
            }
            continue;
        }
        if !trimmed.starts_with("//") && !trimmed.starts_with("#[") {
            kept.push((index + 1, lines[index]));
        }
        index += 1;
    }
    kept
}

#[test]
fn nothing_the_skill_ships_or_the_binary_prints_carries_this_repositorys_history() {
    let root = Path::new(ROOT);
    let mut shipped = vec![root.join("skill/SKILL.md")];
    files_under(&root.join("skill/recipes"), "md", &mut shipped);
    files_under(&root.join("skill/examples"), "dl", &mut shipped);
    assert!(
        shipped.len() >= 4,
        "only {} shipped files found",
        shipped.len()
    );
    let mut sources = Vec::new();
    files_under(&root.join("src"), "rs", &mut sources);
    sources.retain(|path| !TEST_ONLY.iter().any(|skip| path.ends_with(skip)));
    assert!(
        sources.len() >= 20,
        "only {} source files found",
        sources.len()
    );

    let mut hits = Vec::new();
    let mut record = |path: &Path, line: usize, text: &str| {
        for (what, check) in LEAKS {
            if check(text) {
                hits.push(format!(
                    "{}:{line}: {what}: {}",
                    path.strip_prefix(root).unwrap_or(path).display(),
                    text.trim()
                ));
            }
        }
    };
    for path in &shipped {
        let text = fs::read_to_string(path).expect("shipped file reads");
        for (index, line) in text.lines().enumerate() {
            record(path, index + 1, line);
        }
    }
    for path in &sources {
        let text = fs::read_to_string(path).expect("source reads");
        for (line, code) in printable_lines(&text) {
            record(path, line, code);
        }
    }
    assert_eq!(hits, Vec::<String>::new());
}

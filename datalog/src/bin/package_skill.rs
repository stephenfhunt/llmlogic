//! Build the drop-in Claude Code skill bundle.
//!
//! Run via the `cargo package-skill` alias (`.cargo/config.toml`). This is
//! build-tooling, gated behind the `packaging` feature so it never enters a
//! normal `build`/`test`/`clippy` run; it uses only the standard library.
//!
//! It (1) builds the release `datalog` binary, then (2) assembles
//! `target/dist/datalog-skill/` — `SKILL.md`, the compiled `datalog` binary
//! (under that name, so the skill's `./datalog` call resolves to the real
//! binary), the `examples/`, the `recipes/`, and a generated `INSTALL.md` — and
//! (3) rolls a `.tar.gz` if `tar` is available. The bundle is self-contained and
//! installs by being dropped into a `.claude/skills/` directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match package() {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("package-skill: {message}");
            ExitCode::FAILURE
        }
    }
}

fn package() -> Result<String, String> {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    // 1. Build the release engine binary.
    let status = Command::new(&cargo)
        .args(["build", "--release", "--bin", "datalog"])
        .current_dir(&crate_dir)
        .status()
        .map_err(|e| format!("failed to launch cargo: {e}"))?;
    if !status.success() {
        return Err("`cargo build --release --bin datalog` failed".to_string());
    }

    let release_bin = crate_dir.join("target/release/datalog");
    if !release_bin.exists() {
        return Err(format!(
            "expected binary not found: {}",
            release_bin.display()
        ));
    }

    // 2. Assemble target/dist/datalog-skill/ from scratch (deterministic).
    let dist = crate_dir.join("target/dist");
    let bundle = dist.join("datalog-skill");
    if dist.exists() {
        fs::remove_dir_all(&dist).map_err(|e| format!("clearing {}: {e}", dist.display()))?;
    }
    fs::create_dir_all(&bundle).map_err(|e| format!("creating {}: {e}", bundle.display()))?;

    copy(&crate_dir.join("skill/SKILL.md"), &bundle.join("SKILL.md"))?;
    copy(&release_bin, &bundle.join("datalog"))?;
    make_executable(&bundle.join("datalog"))?;
    copy_dir(&crate_dir.join("skill/examples"), &bundle.join("examples"))?;
    copy_dir(&crate_dir.join("skill/recipes"), &bundle.join("recipes"))?;
    fs::write(bundle.join("INSTALL.md"), install_md())
        .map_err(|e| format!("writing INSTALL.md: {e}"))?;

    // 3. Roll a tarball if `tar` is available (best effort).
    let tarball = dist.join("datalog-skill.tar.gz");
    let tar_made = Command::new("tar")
        .args(["-czf"])
        .arg(&tarball)
        .args(["-C"])
        .arg(&dist)
        .arg("datalog-skill")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let mut summary = format!("skill bundle: {}", bundle.display());
    if tar_made {
        summary.push_str(&format!("\ntarball:      {}", tarball.display()));
    } else {
        summary.push_str("\n(tar not available — directory bundle only)");
    }
    Ok(summary)
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    // Markdown carries `<!-- block: name -->` annotations naming paragraphs that
    // an ablation can cut (`experiments/src/harness/ablate.py`). They are
    // structure for tooling, not content, so no consumer ever ships them: this
    // strips them on the way into the bundle, exactly as the harness strips them
    // on the way into a workspace.
    if from.extension().is_some_and(|ext| ext == "md") {
        let text =
            fs::read_to_string(from).map_err(|e| format!("reading {}: {e}", from.display()))?;
        return fs::write(to, strip_block_markers(&text))
            .map_err(|e| format!("writing {}: {e}", to.display()));
    }
    fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("copying {} → {}: {e}", from.display(), to.display()))
}

/// Drops whole lines that are nothing but a `<!-- block: … -->` / `<!-- /block -->`
/// marker, keeping everything between them.
fn strip_block_markers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.trim();
        let is_marker = trimmed.starts_with("<!--")
            && trimmed.ends_with("-->")
            && (trimmed["<!--".len()..].trim_start().starts_with("block:")
                || trimmed["<!--".len()..].trim_start().starts_with("/block"));
        if is_marker {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Recursively copies a directory's contents (one level of files is enough for
/// `examples/`, but this handles nested dirs too).
fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("creating {}: {e}", to.display()))?;
    let entries = fs::read_dir(from).map_err(|e| format!("reading {}: {e}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("reading {}: {e}", from.display()))?;
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &target)?;
        } else {
            copy(&path, &target)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .map_err(|e| format!("stat {}: {e}", path.display()))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).map_err(|e| format!("chmod {}: {e}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn install_md() -> &'static str {
    "\
# Installing the datalog skill

This bundle is a self-contained Claude Code skill: `SKILL.md`, the compiled
`datalog` binary, example programs, and per-use-case recipes.

## Install

Drop the `datalog-skill/` directory into a Claude Code skills directory as
`datalog`:

```sh
# user-level (all projects)
cp -r datalog-skill ~/.claude/skills/datalog

# or project-level
cp -r datalog-skill <project>/.claude/skills/datalog
```

Start a new Claude Code session so the skill is discovered. The skill invokes the
co-located `./datalog` binary — no build step, no PATH entry needed.

## Verify

```sh
cd ~/.claude/skills/datalog
./datalog examples/houses_puzzle.dl        # → solution(3, 1, 2).
```

The binary is built for the platform it was packaged on; rebuild from the
`datalog` source project to target a different platform. Full docs and the
example corpus live in that project.
"
}

#[cfg(test)]
mod tests {
    use super::strip_block_markers;

    #[test]
    fn markers_go_and_the_guidance_stays() {
        let marked =
            "intro\n<!-- block: count-wildcard -->\n  the guidance\n<!-- /block -->\ntail\n";
        assert_eq!(strip_block_markers(marked), "intro\n  the guidance\ntail\n");
    }

    #[test]
    fn an_ordinary_html_comment_is_left_alone() {
        // Only the block vocabulary is tooling. A real comment is content.
        let text = "<!-- a note to a reader -->\nbody\n";
        assert_eq!(strip_block_markers(text), text);
    }

    #[test]
    fn a_marker_indented_inside_a_list_item_still_goes() {
        let text = "- bullet\n  <!-- block: alpha -->\n  guidance\n  <!-- /block -->\n";
        assert_eq!(strip_block_markers(text), "- bullet\n  guidance\n");
    }
}

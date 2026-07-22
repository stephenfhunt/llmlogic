//! Build the drop-in Claude Code skill bundle.
//!
//! Run via the `cargo package-skill` alias (`.cargo/config.toml`). This is
//! build-tooling, gated behind the `packaging` feature so it never enters a
//! normal `build`/`test`/`clippy` run; it uses only the standard library.
//!
//! It (1) builds the release `datalog` binary, then (2) assembles
//! `target/dist/datalog-skill/` — `SKILL.md`, the compiled `datalog` binary
//! (under that name, so the skill's `./datalog` call resolves to the real
//! binary), the `examples/`, and a generated `INSTALL.md` — and (3) rolls a
//! `.tar.gz` if `tar` is available. The bundle is self-contained and installs by
//! being dropped into a `.claude/skills/` directory.

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
    fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("copying {} → {}: {e}", from.display(), to.display()))
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
`datalog` binary, and example programs.

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

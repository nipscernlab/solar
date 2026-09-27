//! `cargo xtask doc-run`: the documentation, executed.
//!
//! Every fenced block in the Markdown of this repository that is tagged `bash`,
//! `powershell` or `cmd` is a claim about what the binary does, and this task runs each
//! one in the shell it is written for, so the documentation can never claim something the
//! binary does not do. A block tagged `no-run` is displayed and skipped, and the
//! convention is that its first line is a comment saying why.
//!
//! Blocks run from the repository root with `target/release` first on the `PATH`, which
//! this task builds first. `powershell` means Windows PowerShell 5.1, the shell whose
//! quoting the README documents, so it and `cmd` only run on Windows; `bash` runs
//! everywhere, through Git Bash on Windows.

use std::path::{Path, PathBuf};
use std::process::Command;

/// One runnable block, with where it came from.
struct Block {
    file: PathBuf,
    line: usize,
    shell: Shell,
    body: String,
    skipped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    clippy::enum_variant_names,
    reason = "the enum is Shell and one shell is really called PowerShell"
)]
enum Shell {
    Bash,
    PowerShell,
    Cmd,
}

impl Shell {
    fn parse(name: &str) -> Option<Shell> {
        match name {
            "bash" => Some(Shell::Bash),
            "powershell" => Some(Shell::PowerShell),
            "cmd" => Some(Shell::Cmd),
            _ => None,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Shell::Bash => "bash",
            Shell::PowerShell => "powershell",
            Shell::Cmd => "cmd",
        }
    }

    /// Whether this shell exists on the machine running the task.
    fn available(self) -> bool {
        match self {
            Shell::Bash => true,
            Shell::PowerShell | Shell::Cmd => cfg!(windows),
        }
    }
}

/// Runs every runnable block, and says what it ran.
///
/// # Errors
///
/// Returns a sentence per failed block, joined, so a run reports every failure at once.
pub(crate) fn run(root: &Path) -> Result<(), String> {
    build_the_binary(root)?;

    let mut blocks = Vec::new();
    for file in markdown_files(root) {
        let text = std::fs::read_to_string(&file)
            .map_err(|failure| format!("{} could not be read: {failure}", file.display()))?;
        collect(&file, &text, &mut blocks);
    }

    let path_with_release = prepend_path(&root.join("target").join("release"));
    let mut failures = Vec::new();
    let (mut ran, mut skipped) = (0usize, 0usize);

    for block in &blocks {
        let place = format!(
            "{}:{} ({})",
            block
                .file
                .strip_prefix(root)
                .unwrap_or(&block.file)
                .display(),
            block.line,
            block.shell.name()
        );
        if block.skipped {
            println!("doc-run: skip {place} (no-run)");
            skipped += 1;
            continue;
        }
        if !block.shell.available() {
            println!("doc-run: skip {place} (shell not on this system)");
            skipped += 1;
            continue;
        }

        match execute(root, &path_with_release, block) {
            Ok(()) => {
                println!("doc-run: ok   {place}");
                ran += 1;
            }
            Err(why) => {
                println!("doc-run: FAIL {place}");
                failures.push(format!("{place}: {why}"));
            }
        }
    }

    println!(
        "doc-run: {ran} ran, {skipped} skipped, {} failed",
        failures.len()
    );
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// The release binary the blocks call.
fn build_the_binary(root: &Path) -> Result<(), String> {
    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--locked",
            "--package",
            "solar-cli",
            "--quiet",
        ])
        .current_dir(root)
        // The same flags as every other xtask build, so alternating xtask commands never
        // rebuild the world over a flag change.
        .env("RUSTFLAGS", crate::flags::remap_rustflags(root))
        .status()
        .map_err(|failure| format!("cargo could not be started: {failure}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("the release build failed, so the documentation cannot be run".to_owned())
    }
}

/// Every Markdown file the documentation lives in.
fn markdown_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    walk(&root.join("docs"), &mut files);
    files.sort();
    files
}

/// Collects the Markdown under a directory, recursively.
fn walk(directory: &Path, into: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, into);
        } else if path.extension().is_some_and(|extension| extension == "md") {
            into.push(path);
        }
    }
}

/// Reads the fenced blocks of one file into `into`.
fn collect(file: &Path, text: &str, into: &mut Vec<Block>) {
    let mut lines = text.lines().enumerate();
    while let Some((index, line)) = lines.next() {
        let Some(fence_info) = line.strip_prefix("```") else {
            continue;
        };
        let mut words = fence_info.split_whitespace();
        let language = words.next().unwrap_or("");
        let skipped = words.any(|word| word == "no-run");

        let mut body = String::new();
        for (_, inner) in lines.by_ref() {
            if inner.trim_end() == "```" {
                break;
            }
            body.push_str(inner);
            body.push('\n');
        }

        if let Some(shell) = Shell::parse(language) {
            into.push(Block {
                file: file.to_path_buf(),
                line: index + 1,
                shell,
                body,
                skipped,
            });
        }
    }
}

/// The `PATH` of the blocks: the release directory first, then what was there.
fn prepend_path(release: &Path) -> std::ffi::OsString {
    let mut paths = vec![release.to_path_buf()];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(paths).unwrap_or_default()
}

/// Runs one block in its shell, from the repository root.
fn execute(root: &Path, path: &std::ffi::OsStr, block: &Block) -> Result<(), String> {
    let directory = root.join("target").join("doc-run");
    std::fs::create_dir_all(&directory)
        .map_err(|failure| format!("{} could not be created: {failure}", directory.display()))?;

    let mut command = match block.shell {
        Shell::Bash => {
            let script = directory.join("block.sh");
            let body = format!("set -euo pipefail\n{}", block.body);
            std::fs::write(&script, body)
                .map_err(|failure| format!("the script could not be written: {failure}"))?;
            let mut command = Command::new("bash");
            command.arg(script);
            command
        }
        Shell::PowerShell => {
            let script = directory.join("block.ps1");
            // Stop on the first cmdlet error; native commands are checked at the end,
            // which covers the short blocks the documentation holds.
            let body = format!(
                "$ErrorActionPreference = \"Stop\"\n{}\nif ($LASTEXITCODE -ne $null -and $LASTEXITCODE -ne 0) {{ exit $LASTEXITCODE }}",
                block.body
            );
            std::fs::write(&script, body)
                .map_err(|failure| format!("the script could not be written: {failure}"))?;
            let mut command = Command::new("powershell");
            command.args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ]);
            command.arg(script);
            command
        }
        Shell::Cmd => {
            let script = directory.join("block.cmd");
            let mut body = String::from("@echo off\r\n");
            for line in block.body.lines().filter(|line| !line.trim().is_empty()) {
                body.push_str(line);
                body.push_str("\r\nif errorlevel 1 exit /b 1\r\n");
            }
            std::fs::write(&script, body)
                .map_err(|failure| format!("the script could not be written: {failure}"))?;
            let mut command = Command::new("cmd");
            command.arg("/C");
            command.arg(script);
            command
        }
    };

    let output = command
        .current_dir(root)
        .env("PATH", path)
        .env_remove("SOLAR_LOG")
        .env_remove("SOLAR_LOG_FORMAT")
        .output()
        .map_err(|failure| format!("{} could not be started: {failure}", block.shell.name()))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "exited with {:?}\n--- the block ---\n{}--- stdout ---\n{}--- stderr ---\n{}",
            output.status.code(),
            block.body,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ))
    }
}

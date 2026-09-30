#!/usr/bin/env -S cargo -Zscript
---
[package]
edition = "2024"
---
// SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Run every check this repository has, using whatever is on `$PATH`.
//!
//! ```console
//! $ scripts/check-all.rs [REFERENCE_COMPILER]
//! ```
//!
//! Given the course's cflat binary, the one supporting `-o tokens`, the lexer
//! cross-validation and the fuzzer's token-stream comparison run too.  Run it
//! inside `nix develop`, which provides every tool.  `nix flake check` is the
//! hermetic equivalent, and all that CI runs.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

const BOLD: &str = "\x1b[1m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const PLAIN: &str = "\x1b[0m";

/// Parent of this script's directory, which cargo reports as the manifest dir
fn repository_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("scripts/ lives inside the repository")
}

fn script(name: &str) -> String {
    repository_root()
        .join("scripts")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn command(program: &str, arguments: &[&str]) -> Command {
    let mut command = Command::new(program);
    command.args(arguments).current_dir(repository_root());
    command
}

fn quiet(program: &str, arguments: &[&str]) -> bool {
    command(program, arguments)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn capture(program: &str, arguments: &[&str]) -> String {
    command(program, arguments)
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default()
}

#[derive(Default)]
struct Sweep {
    failures: Vec<String>,
}

impl Sweep {
    fn heading(name: &str) {
        println!("\n{BOLD}== {name}{PLAIN}");
    }

    fn skipped(what: &str) {
        println!("{YELLOW}   skipped ({what} not available){PLAIN}");
    }

    fn fail(&mut self, name: &str, why: &str) {
        println!("{RED}   FAILED{why}{PLAIN}");
        self.failures.push(name.to_owned());
    }

    fn record(&mut self, name: &str, ok: bool) {
        if ok {
            println!("{GREEN}   ok{PLAIN}");
        } else {
            self.fail(name, "");
        }
    }

    /// Run a command from the repository root and record the outcome
    fn step(&mut self, name: &str, program: &str, arguments: &[&str]) {
        Self::heading(name);
        let ok = command(program, arguments)
            .status()
            .is_ok_and(|status| status.success());
        self.record(name, ok);
    }
}

/// `tree-sitter generate` has just run, so any difference from the committed
/// parser means `src/` was stale.  This compares against HEAD rather than the
/// index, because a plain `git diff` sees nothing once the regenerated files
/// are staged and would pass for entirely the wrong reason.
fn generated_parser_is_current(sweep: &mut Sweep) {
    let name = "generated parser is current";
    Sweep::heading(name);

    if !quiet("git", &["rev-parse", "--verify", "--quiet", "HEAD"]) {
        return Sweep::skipped("a commit to compare against");
    }
    let dirty = capture("git", &["status", "--porcelain", "--", "src/"]);
    if dirty.trim().is_empty() {
        return sweep.record(name, true);
    }
    sweep.fail(
        name,
        ": src/ differs from HEAD; commit the regenerated parser",
    );
    dirty.lines().for_each(|line| println!("     {line}"));
}

/// Lint every corpus program, which must come out clean except for the
/// `over-generation` ones that the linter exists to reject.
fn linter_against_the_corpus(sweep: &mut Sweep, scratch: &Path) {
    let name = "linter against the corpus";
    Sweep::heading(name);

    if !quiet(
        "cargo",
        &["build", "--quiet", "--release", "--example", "lint"],
    ) {
        return sweep.fail(name, " to build the linter");
    }
    let corpus = script("corpus-sources.rs");
    if !quiet("cargo", &["-Zscript", &corpus, &scratch.to_string_lossy()]) {
        return sweep.fail(name, " to extract the corpus");
    }

    let lint = repository_root().join("target/release/examples/lint");
    let mut files: Vec<PathBuf> = fs::read_dir(scratch)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "cb"))
        .collect();
    files.sort();

    let complaints: Vec<String> = files
        .iter()
        .filter_map(|file| {
            let base = file.file_name().unwrap_or_default().to_string_lossy();
            let output = Command::new(&lint).arg(file).output().ok();
            let clean = output
                .as_ref()
                .is_some_and(|output| output.status.success());
            match base.strip_prefix("over-generation--") {
                // Unknown type names need a symbol table, beyond the linter
                Some(rest) if rest.starts_with("unknown-type-names") => None,
                Some(_) => clean.then(|| format!("   linter missed {base}")),
                None if clean => None,
                None => Some(format!(
                    "   linter wrongly flagged {base}:\n{}",
                    output
                        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
                        .unwrap_or_default()
                        .lines()
                        .map(|line| format!("     {line}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                )),
            }
        })
        .collect();
    complaints
        .iter()
        .for_each(|complaint| println!("{complaint}"));
    sweep.record(name, complaints.is_empty());
}

fn main() -> ExitCode {
    let reference = env::args()
        .nth(1)
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .map(|path| path.to_string_lossy().into_owned());
    let mut sweep = Sweep::default();

    sweep.step("rustfmt", "cargo", &["fmt", "--check"]);
    sweep.step(
        "rustfmt scripts",
        "sh",
        &["-c", "rustfmt --edition 2024 --check scripts/*.rs"],
    );
    sweep.step(
        "clippy",
        "cargo",
        &["clippy", "--all-targets", "--", "-D", "warnings"],
    );
    sweep.step("tree-sitter generate", "tree-sitter", &["generate"]);
    generated_parser_is_current(&mut sweep);
    sweep.step("corpus + highlight tests", "tree-sitter", &["test"]);
    sweep.step("cargo test", "cargo", &["test", "--all-targets"]);
    sweep.step("cargo doc tests", "cargo", &["test", "--doc"]);
    sweep.step("reuse lint", "reuse", &["lint"]);

    let emacs_has_tree_sitter = quiet(
        "emacs",
        &[
            "-Q",
            "--batch",
            "--eval",
            "(kill-emacs (if (treesit-available-p) 0 1))",
        ],
    );
    if emacs_has_tree_sitter {
        sweep.step(
            "emacs: byte-compile + ERT",
            "make",
            &["-C", "emacs", "test"],
        );
        sweep.step("emacs: indent stress", "make", &["-C", "emacs", "stress"]);
    } else {
        Sweep::heading("emacs");
        Sweep::skipped("emacs with tree-sitter support");
    }

    let scratch = env::temp_dir().join(format!("cflat-check-{}", std::process::id()));
    let _ = fs::create_dir_all(&scratch);
    linter_against_the_corpus(&mut sweep, &scratch);
    let _ = fs::remove_dir_all(&scratch);

    match &reference {
        Some(path) => sweep.step(
            "lexer cross-validation",
            "cargo",
            &["-Zscript", &script("cross-validate.rs"), path],
        ),
        None => {
            Sweep::heading("lexer cross-validation");
            Sweep::skipped("reference compiler");
        }
    }

    let helpers = [
        "build",
        "--quiet",
        "--release",
        "--example",
        "tokens",
        "--example",
        "parse-check",
    ];
    if quiet("cargo", &helpers) {
        let fuzz = script("fuzz.rs");
        let seed = std::process::id().to_string();
        let arguments: Vec<&str> = ["-Zscript", &fuzz, "-n", "500", "--seed", &seed]
            .into_iter()
            .chain(
                reference
                    .iter()
                    .flat_map(|path| ["--reference", path.as_str()]),
            )
            .collect();
        sweep.step("fuzz", "cargo", &arguments);
    } else {
        Sweep::heading("fuzz");
        sweep.fail("fuzz", " to build the token dumper");
    }

    println!();
    if sweep.failures.is_empty() {
        println!("{GREEN}all checks passed{PLAIN}");
        return ExitCode::SUCCESS;
    }
    println!("{RED}{} check(s) failed:{PLAIN}", sweep.failures.len());
    sweep
        .failures
        .iter()
        .for_each(|failure| println!("  - {failure}"));
    ExitCode::FAILURE
}

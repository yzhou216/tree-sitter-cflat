#!/usr/bin/env -S cargo -Zscript
---
[package]
edition = "2024"
---
// SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Diff this grammar's token stream against the reference C♭ compiler's lexer.
//!
//! The course compiler's hand-written lexer is the authoritative implementation
//! of C♭'s lexical layer, so agreeing with it token for token is the strongest
//! available check on `grammar.js`'s `extras`, keyword extraction and comment
//! patterns.
//!
//! ```console
//! $ scripts/cross-validate.rs REFERENCE_COMPILER [FILE...]
//! ```
//!
//! REFERENCE_COMPILER is the cflat binary that supports `-o tokens`.  With no
//! files, the grammar's own corpus is extracted and used, minus the `:error`
//! tests.  Those are left out because tree-sitter's lexer only considers the
//! tokens legal in the current parse state, so on input the grammar rejects
//! it can legitimately disagree with a standalone lexer, for instance by
//! reading a misplaced `let` as an identifier.

use std::{
    env, fs, io, iter,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

/// Parent of this script's directory, which cargo reports as the manifest dir
fn repository_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("scripts/ lives inside the repository")
}

fn tokens_of(program: &Path, arguments: &[&Path]) -> String {
    Command::new(program)
        .args(arguments)
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default()
}

/// First few places where two token streams disagree
fn differences(ours: &str, theirs: &str) -> Vec<String> {
    let ours: Vec<&str> = ours.lines().collect();
    let theirs: Vec<&str> = theirs.lines().collect();
    (0..ours.len().max(theirs.len()))
        .filter(|&line| ours.get(line) != theirs.get(line))
        .take(5)
        .map(|line| {
            format!(
                "  line {}:\n    tree-sitter: {}\n    reference:   {}",
                line + 1,
                ours.get(line).unwrap_or(&"<end of stream>"),
                theirs.get(line).unwrap_or(&"<end of stream>"),
            )
        })
        .collect()
}

fn corpus_files(into: &Path) -> io::Result<Vec<PathBuf>> {
    let extracted = Command::new("cargo")
        .arg("-Zscript")
        .arg(repository_root().join("scripts/corpus-sources.rs"))
        .arg(into)
        .stdout(Stdio::null())
        .status()?
        .success();
    if !extracted {
        return Err(io::Error::other("could not extract the corpus"));
    }

    let mut files: Vec<PathBuf> = fs::read_dir(into)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "cb"))
        .collect();
    files.sort();
    Ok(files)
}

fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let Some(reference) = arguments.next().map(PathBuf::from) else {
        eprintln!("usage: cross-validate.rs REFERENCE_COMPILER [FILE...]");
        return ExitCode::from(2);
    };
    if !reference.is_file() {
        eprintln!("not a file: {}", reference.display());
        return ExitCode::from(2);
    }

    let root = repository_root();
    let built = Command::new("cargo")
        .args(["build", "--quiet", "--release", "--example", "tokens"])
        .arg("--manifest-path")
        .arg(root.join("Cargo.toml"))
        .status()
        .is_ok_and(|status| status.success());
    if !built {
        eprintln!("failed to build the tokens example");
        return ExitCode::from(2);
    }
    let ours = root.join("target/release/examples/tokens");

    let given: Vec<PathBuf> = arguments.map(PathBuf::from).collect();
    let scratch = env::temp_dir().join(format!("cflat-corpus-{}", std::process::id()));
    let files = if given.is_empty() {
        match corpus_files(&scratch) {
            Ok(files) => files,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(2);
            }
        }
    } else {
        given
    };

    let mismatches: Vec<String> = files
        .iter()
        .filter_map(|file| {
            let mine = tokens_of(&ours, &[file]);
            let theirs = tokens_of(&reference, &[Path::new("-o"), Path::new("tokens"), file]);
            (mine != theirs).then(|| {
                iter::once(format!("MISMATCH: {}", file.display()))
                    .chain(differences(&mine, &theirs))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        })
        .collect();
    let _ = fs::remove_dir_all(&scratch);

    mismatches
        .iter()
        .for_each(|mismatch| println!("{mismatch}"));
    println!(
        "\ncross-validation: {} matched, {} differed ({} files)",
        files.len() - mismatches.len(),
        mismatches.len(),
        files.len()
    );
    if mismatches.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

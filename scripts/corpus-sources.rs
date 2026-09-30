#!/usr/bin/env -S cargo -Zscript
---
[package]
edition = "2024"
---
// SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Extract every source snippet from `test/corpus/*.txt` into standalone files.
//!
//! The corpus is the grammar's own test suite, which also makes it a ready-made
//! body of C♭ to feed to other tools, notably the reference lexer through
//! `scripts/cross-validate.rs`.
//!
//! ```console
//! $ scripts/corpus-sources.rs OUTPUT_DIR [--include-errors] [--corpus DIR]
//! ```
//!
//! `:error` tests are skipped unless asked for, since they are not C♭ and the
//! reference parser cannot be expected to agree about them.  Their token
//! streams are still well defined, which is what `--include-errors` is for.

use std::{
    env, fs, io, iter,
    path::{Path, PathBuf},
    process::ExitCode,
};

struct Options {
    output: PathBuf,
    corpus: PathBuf,
    include_errors: bool,
}

/// Parent of this script's directory, which cargo reports as the manifest dir
fn repository_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("scripts/ lives inside the repository")
}

fn parse_arguments() -> Option<Options> {
    let mut output = None;
    let mut corpus = None;
    let mut include_errors = false;
    let mut arguments = env::args().skip(1);

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--include-errors" => include_errors = true,
            "--corpus" => corpus = Some(PathBuf::from(arguments.next()?)),
            _ if argument.starts_with('-') || output.is_some() => return None,
            _ => output = Some(PathBuf::from(argument)),
        }
    }

    Some(Options {
        output: output?,
        corpus: corpus.unwrap_or_else(|| repository_root().join("test/corpus")),
        include_errors,
    })
}

struct Test {
    name: String,
    is_error: bool,
    source: String,
}

fn is_rule(line: &str, character: char) -> bool {
    let line = line.trim_end();
    line.len() >= 10 && line.chars().all(|found| found == character)
}

fn is_attribute(line: &str) -> bool {
    line.strip_prefix(':')
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_lowercase()))
}

/// Split one corpus file into its tests.
///
/// Each test is a `====` header block holding its name and any `:attributes`,
/// then the source, then a `----` divider, then the expected tree.
fn split(text: &str) -> Vec<Test> {
    let mut lines = text.lines();
    iter::from_fn(|| {
        lines.find(|line| is_rule(line, '='))?;
        let header: Vec<&str> = lines
            .by_ref()
            .take_while(|line| !is_rule(line, '='))
            .map(str::trim)
            .collect();
        let body: Vec<&str> = lines
            .by_ref()
            .take_while(|line| !is_rule(line, '-'))
            .collect();

        Some(Test {
            name: header
                .iter()
                .filter(|line| !is_attribute(line))
                .copied()
                .collect::<Vec<_>>()
                .join(" "),
            is_error: header.contains(&":error"),
            source: body.join("\n").trim_matches('\n').to_owned() + "\n",
        })
    })
    .filter(|test| !test.source.trim().is_empty())
    .collect()
}

fn slugify(name: &str) -> String {
    let lowered: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = lowered
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "unnamed".to_owned()
    } else {
        slug
    }
}

/// Write every wanted test to its own file and return how many were written
/// and how many `:error` tests were skipped.
fn extract(options: &Options) -> io::Result<(usize, usize)> {
    let mut files: Vec<PathBuf> = fs::read_dir(&options.corpus)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "txt"))
        .collect();
    files.sort();

    fs::create_dir_all(&options.output)?;

    files.iter().try_fold((0, 0), |(written, skipped), file| {
        let stem = file.file_stem().unwrap_or_default().to_string_lossy();
        split(&fs::read_to_string(file)?).into_iter().try_fold(
            (written, skipped),
            |(written, skipped), test| {
                if test.is_error && !options.include_errors {
                    return Ok((written, skipped + 1));
                }
                let name = format!("{stem}--{}.cb", slugify(&test.name));
                fs::write(options.output.join(name), &test.source)?;
                Ok((written + 1, skipped))
            },
        )
    })
}

fn main() -> ExitCode {
    let Some(options) = parse_arguments() else {
        eprintln!("usage: corpus-sources.rs OUTPUT_DIR [--include-errors] [--corpus DIR]");
        return ExitCode::FAILURE;
    };

    if !options.corpus.is_dir() {
        eprintln!("no corpus directory at {}", options.corpus.display());
        return ExitCode::FAILURE;
    }

    match extract(&options) {
        Ok((written, skipped)) => {
            println!(
                "wrote {written} files to {} ({skipped} error tests skipped)",
                options.output.display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

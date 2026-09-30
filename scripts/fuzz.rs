#!/usr/bin/env -S cargo -Zscript
---
[package]
edition = "2024"
---
// SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Generate random C♭ programs and check the grammar against them.
//!
//! Valid programs come straight from the reference CFG.  Every one must parse
//! with no ERROR or MISSING node, and given a reference compiler its lexer's
//! token stream must match ours exactly.  Each is then corrupted in a small
//! way, and the mutant need not parse but must not hang or crash the parser.
//!
//! ```console
//! $ cargo build --release --example tokens --example parse-check
//! $ scripts/fuzz.rs [-n COUNT] [--seed SEED] [--reference PATH] [--keep DIR] [--dump DIR]
//! ```

#![feature(default_field_values)]

use std::{
    env, fs, io, iter,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

/// SplitMix64, so `--seed` reproduces a run on any machine
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mixed = (self.0 ^ (self.0 >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        let mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        mixed ^ (mixed >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    fn between(&mut self, low: usize, high: usize) -> usize {
        low + self.below(high - low + 1)
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn chance(&mut self, probability: f64) -> bool {
        self.unit() < probability
    }

    fn pick<'a, T>(&mut self, options: &'a [T]) -> &'a T {
        &options[self.below(options.len())]
    }

    fn character(&mut self, alphabet: &str) -> char {
        alphabet.as_bytes()[self.below(alphabet.len())] as char
    }

    fn string(&mut self, alphabet: &str, length: usize) -> String {
        (0..length).map(|_| self.character(alphabet)).collect()
    }
}

const KEYWORDS: [&str; 16] = [
    "and", "or", "not", "nil", "new", "int", "if", "else", "while", "break", "continue", "return",
    "let", "fn", "struct", "extern",
];
const BINARY: [&str; 12] = [
    "+", "-", "*", "/", "==", "!=", "<", "<=", ">", ">=", "and", "or",
];
const UNARY: [&str; 2] = ["-", "not"];

/// `body` with every `*/` removed, so it cannot close a block comment early.
///
/// One pass of `replace` is not enough, because deleting the `*/` in `**//`
/// splices its neighbours into a fresh one.
fn without_closer(mut body: String) -> String {
    while body.contains("*/") {
        body = body.replace("*/", "");
    }
    body
}

/// Programs drawn from the C♭ reference CFG
struct Generator {
    rng: Rng,
    names: Vec<String>,
}

impl Generator {
    fn identifier(&mut self) -> String {
        const HEAD: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
        const TAIL: &str = "abcdefghijklmnopqrstuvwxyz0123456789_";
        iter::repeat_with(|| {
            let length = self.rng.between(0, 6);
            format!(
                "{}{}",
                self.rng.character(HEAD),
                self.rng.string(TAIL, length)
            )
        })
        .find(|name| !KEYWORDS.contains(&name.as_str()))
        .expect("repeat_with never ends")
    }

    fn number(&mut self) -> String {
        let digits = self.rng.between(1, 6);
        self.rng.below(10usize.pow(digits as u32)).to_string()
    }

    fn comment(&mut self) -> String {
        let length = self.rng.between(0, 20);
        let body = self.rng.string("abc xyz*/-+", length);
        if self.rng.chance(0.5) {
            format!("// {body}\n")
        } else {
            format!("/* {} */", without_closer(body))
        }
    }

    fn maybe_comment(&mut self) -> String {
        if self.rng.chance(0.12) {
            self.comment()
        } else {
            String::new()
        }
    }

    fn ty(&mut self, depth: usize) -> String {
        let forms: &[&str] = if depth < 3 {
            &["int", "id", "pointer", "array", "function"]
        } else {
            &["int", "id"]
        };
        match *self.rng.pick(forms) {
            "int" => "int".to_owned(),
            "id" if self.names.is_empty() => self.identifier(),
            "id" => self.rng.pick(&self.names).clone(),
            "pointer" => format!("&{}", self.ty(depth + 1)),
            "array" => format!("[{}]", self.ty(depth + 1)),
            _ => self.function_type(depth),
        }
    }

    fn function_type(&mut self, depth: usize) -> String {
        let parameters = self.joined(", ", 0, 3, |generator| generator.ty(depth + 1));
        format!("({parameters}) -> {}", self.ty(depth + 1))
    }

    fn declaration(&mut self) -> String {
        format!("{}: {}", self.identifier(), self.ty(0))
    }

    fn expression(&mut self, depth: usize) -> String {
        if depth >= 4 {
            return match self.rng.below(3) {
                0 => self.identifier(),
                1 => self.number(),
                _ => "nil".to_owned(),
            };
        }

        const FORMS: [&str; 10] = [
            "id", "num", "nil", "new", "array", "paren", "unary", "binary", "ternary", "postfix",
        ];
        match *self.rng.pick(&FORMS) {
            "id" => self.identifier(),
            "num" => self.number(),
            "nil" => "nil".to_owned(),
            "new" => format!("new {}", self.ty(depth + 1)),
            "array" => format!("[{}; {}]", self.ty(depth + 1), self.expression(depth + 1)),
            "paren" => format!("({})", self.expression(depth + 1)),
            "unary" => format!("{} {}", self.rng.pick(&UNARY), self.expression(depth + 1)),
            "binary" => {
                let left = self.expression(depth + 1);
                let operator = self.rng.pick(&BINARY);
                format!("{left} {operator} {}", self.expression(depth + 1))
            }
            "ternary" => {
                let condition = self.expression(depth + 1);
                let consequence = self.expression(depth + 1);
                format!(
                    "{condition} ? {consequence} : {}",
                    self.expression(depth + 1)
                )
            }
            _ => self.postfix(depth),
        }
    }

    fn postfix(&mut self, depth: usize) -> String {
        let base = self.expression(depth + 1);
        let count = self.rng.between(1, 2);
        (0..count).fold(base, |base, _| {
            match *self.rng.pick(&["call", "index", "field", "deref"]) {
                "call" => {
                    let arguments =
                        self.joined(", ", 0, 3, |generator| generator.expression(depth + 2));
                    format!("{base}({arguments})")
                }
                "index" => format!("{base}[{}]", self.expression(depth + 2)),
                "field" => format!("{base}.{}", self.identifier()),
                _ => format!("{base}.*"),
            }
        })
    }

    fn statement(&mut self, depth: usize) -> String {
        let forms: &[&str] = if depth < 3 {
            &[
                "expression",
                "assignment",
                "break",
                "continue",
                "return",
                "if",
                "while",
            ]
        } else {
            &["expression", "assignment", "break", "continue", "return"]
        };
        let form = *self.rng.pick(forms);
        let lead = self.maybe_comment();

        match form {
            "expression" => format!("{lead}{};", self.expression(0)),
            "assignment" => {
                let left = self.expression(0);
                format!("{lead}{left} = {};", self.expression(0))
            }
            "break" => format!("{lead}break;"),
            "continue" => format!("{lead}continue;"),
            "return" => format!("{lead}return {};", self.expression(0)),
            "while" => {
                let condition = self.expression(0);
                format!("{lead}while {condition} {}", self.block(depth + 1))
            }
            _ => {
                let condition = self.expression(0);
                let consequence = self.block(depth + 1);
                let alternative = if self.rng.chance(0.5) {
                    format!(" else {}", self.block(depth + 1))
                } else {
                    String::new()
                };
                format!("{lead}if {condition} {consequence}{alternative}")
            }
        }
    }

    fn block(&mut self, depth: usize) -> String {
        let statements = self.joined(" ", 0, 3, |generator| generator.statement(depth));
        format!("{{ {statements} }}")
    }

    fn struct_declaration(&mut self) -> String {
        let name = self.identifier();
        self.names.push(name.clone());
        let fields = self.joined(", ", 0, 4, Self::declaration);
        format!("struct {name} {{ {fields} }}")
    }

    fn extern_declaration(&mut self) -> String {
        let name = self.identifier();
        format!("extern {name} : {};", self.function_type(0))
    }

    fn function(&mut self) -> String {
        let name = self.identifier();
        let parameters = self.joined(", ", 0, 3, Self::declaration);
        let lets = self.joined(" ", 0, 2, |generator| {
            format!("let {};", generator.joined(", ", 0, 3, Self::declaration))
        });
        let statements = self.joined(" ", 0, 4, |generator| generator.statement(0));
        let body = format!("{lets} {statements}");
        format!(
            "fn {name}({parameters}) -> {} {{ {} }}",
            self.ty(0),
            body.trim()
        )
    }

    /// Comment that runs to end of input.
    ///
    /// The grammar reports this lexing error as an `unterminated_comment`
    /// extra, so the program around it still parses, and the reference lexer
    /// emits a single `Error` token over the same span, so the two must still
    /// agree.
    fn unterminated_comment(&mut self) -> String {
        let length = self.rng.between(0, 30);
        let body = self.rng.string("abc xyz*-+\n", length);
        if self.rng.chance(0.5) {
            format!("// {}", body.replace('\n', " "))
        } else {
            format!("/* {}", without_closer(body))
        }
    }

    fn program(&mut self) -> String {
        self.names.clear();
        let lead = self.maybe_comment();
        let items = self.joined("\n", 1, 4, |generator| match generator.rng.below(3) {
            0 => generator.struct_declaration(),
            1 => generator.extern_declaration(),
            _ => generator.function(),
        });
        let mut source = format!("{lead}{items}\n");
        if self.rng.chance(0.12) {
            source += &self.unterminated_comment();
        }

        // A line comment ends only at `\n`, so with lone-CR line endings one
        // would run to end of input and swallow the program.  The reference
        // lexer agrees, but the result is no longer valid C♭, so lone CRs are
        // used only when there is no line comment.  CRLF is always safe, since
        // it still contains the `\n`.
        let roll = self.rng.unit();
        if roll < 0.06 {
            source.replace('\n', "\r\n")
        } else if roll < 0.10 && !source.contains("//") {
            source.replace('\n', "\r")
        } else {
            source
        }
    }

    fn joined(
        &mut self,
        separator: &str,
        low: usize,
        high: usize,
        mut each: impl FnMut(&mut Self) -> String,
    ) -> String {
        let count = self.rng.between(low, high);
        (0..count)
            .map(|_| each(self))
            .collect::<Vec<_>>()
            .join(separator)
    }
}

/// Corrupt `source` in a small, arbitrary way
fn mutate(source: &str, rng: &mut Rng) -> String {
    if source.is_empty() {
        return "?".to_owned();
    }
    let mut characters: Vec<char> = source.chars().collect();
    for _ in 0..rng.between(1, 4) {
        if characters.is_empty() {
            break;
        }
        let index = rng.below(characters.len());
        match rng.below(4) {
            0 => drop(characters.remove(index)),
            1 => characters.insert(index, characters[index]),
            2 => characters[index] = rng.character("{}()[]<>;,.:?*/&=+-\"\\ \n\tabc09"),
            _ if index + 1 < characters.len() => characters.swap(index, index + 1),
            _ => {}
        }
    }
    characters.into_iter().collect()
}

fn run(program: &Path, arguments: &[&Path]) -> io::Result<Output> {
    Command::new(program).args(arguments).output()
}

/// Like [`run`], but kills the child if it overruns and returns `None`.
///
/// This is only for mutated input, where the point is that the parser
/// terminates at all.  The helpers print one short line per file, so polling
/// cannot deadlock on a full pipe.
fn run_bounded(program: &Path, argument: &Path, limit: Duration) -> io::Result<Option<Output>> {
    let mut child = Command::new(program)
        .arg(argument)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let deadline = Instant::now() + limit;
    while child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(10));
    }
    child.wait_with_output().map(Some)
}

/// Parent of this script's directory, which cargo reports as the manifest dir
fn repository_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("scripts/ lives inside the repository")
}

fn helpers() -> PathBuf {
    repository_root().join("target/release/examples")
}

struct Options {
    count: usize = 200,
    seed: u64 = 0,
    reference: Option<PathBuf> = None,
    keep: Option<PathBuf> = None,
    dump: Option<PathBuf> = None,
}

fn parse_arguments() -> Option<Options> {
    let mut options = Options { .. };
    let mut arguments = env::args().skip(1);

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-n" | "--count" => options.count = arguments.next()?.parse().ok()?,
            "--seed" => options.seed = arguments.next()?.parse().ok()?,
            "--reference" => options.reference = Some(PathBuf::from(arguments.next()?)),
            "--keep" => options.keep = Some(PathBuf::from(arguments.next()?)),
            "--dump" => options.dump = Some(PathBuf::from(arguments.next()?)),
            _ => return None,
        }
    }
    Some(options)
}

struct Failure {
    kind: &'static str,
    source: String,
    detail: String,
}

/// Programs that passed each check
#[derive(Default)]
struct Tallies {
    parsed: usize,
    matched: usize,
    survived: usize,
}

fn fuzz(options: &Options, scratch: &Path) -> io::Result<(Vec<Failure>, Tallies)> {
    let tokens = helpers().join("tokens");
    let parse_check = helpers().join("parse-check");

    let mut generator = Generator {
        rng: Rng(options.seed),
        names: Vec::new(),
    };
    let mut failures = Vec::new();
    let mut tallies = Tallies::default();
    let case = scratch.join("case.cb");
    let broken = scratch.join("broken.cb");

    for iteration in 0..options.count {
        let source = generator.program();
        fs::write(&case, &source)?;
        if let Some(dump) = &options.dump {
            fs::write(dump.join(format!("case-{iteration:06}.cb")), &source)?;
        }

        let parsed = run(&parse_check, &[&case])?;
        if parsed.status.success() {
            tallies.parsed += 1;
        } else {
            failures.push(Failure {
                kind: "parse",
                source: source.clone(),
                detail: String::from_utf8_lossy(&parsed.stdout).into_owned(),
            });
        }

        if let Some(reference) = &options.reference {
            let ours = run(&tokens, &[&case])?;
            let theirs = run(reference, &[Path::new("-o"), Path::new("tokens"), &case])?;
            if ours.stdout == theirs.stdout {
                tallies.matched += 1;
            } else {
                failures.push(Failure {
                    kind: "tokens",
                    source: source.clone(),
                    detail: format!(
                        "ours:\n{}\ntheirs:\n{}",
                        String::from_utf8_lossy(&ours.stdout),
                        String::from_utf8_lossy(&theirs.stdout)
                    ),
                });
            }
        }

        let corrupted = mutate(&source, &mut generator.rng);
        fs::write(&broken, &corrupted)?;
        match run_bounded(&parse_check, &broken, Duration::from_secs(30))? {
            // Exit code 1 only means the mutant has errors
            Some(result) if matches!(result.status.code(), Some(0 | 1)) => tallies.survived += 1,
            Some(result) => failures.push(Failure {
                kind: "mutated",
                source: corrupted,
                detail: format!("exit {:?}", result.status.code()),
            }),
            None => failures.push(Failure {
                kind: "timeout",
                source: corrupted,
                detail: "parser did not terminate".to_owned(),
            }),
        }

        if (iteration + 1) % 50 == 0 {
            println!("  ... {}/{}", iteration + 1, options.count);
        }
    }
    Ok((failures, tallies))
}

fn main() -> ExitCode {
    let Some(options) = parse_arguments() else {
        eprintln!(
            "usage: fuzz.rs [-n COUNT] [--seed SEED] [--reference PATH] [--keep DIR] [--dump DIR]"
        );
        return ExitCode::FAILURE;
    };

    let missing: Vec<&str> = ["tokens", "parse-check"]
        .into_iter()
        .filter(|tool| !helpers().join(tool).exists())
        .collect();
    if !missing.is_empty() {
        eprintln!(
            "build the helpers first:\n    \
             cargo build --release --example tokens --example parse-check\n\
             (missing: {})",
            missing.join(", ")
        );
        return ExitCode::from(2);
    }

    if let Some(dump) = &options.dump
        && let Err(error) = fs::create_dir_all(dump)
    {
        eprintln!("{error}");
        return ExitCode::FAILURE;
    }

    let scratch = env::temp_dir().join(format!("cflat-fuzz-{}", std::process::id()));
    if let Err(error) = fs::create_dir_all(&scratch) {
        eprintln!("{error}");
        return ExitCode::FAILURE;
    }
    let outcome = fuzz(&options, &scratch);
    let _ = fs::remove_dir_all(&scratch);

    let (failures, tallies) = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    println!();
    println!(
        "valid programs parsed cleanly: {}/{}",
        tallies.parsed, options.count
    );
    if options.reference.is_some() {
        println!(
            "token streams matched:         {}/{}",
            tallies.matched, options.count
        );
    }
    println!(
        "mutated programs survived:     {}/{}",
        tallies.survived, options.count
    );

    if failures.is_empty() {
        println!("\nno failures");
        return ExitCode::SUCCESS;
    }

    println!("\n{} FAILURES", failures.len());
    if let Some(keep) = &options.keep {
        let _ = fs::create_dir_all(keep);
    }
    for (index, failure) in failures.iter().take(10).enumerate() {
        let detail: String = failure.detail.chars().take(800).collect();
        println!(
            "\n--- {} failure {index} ---\n{}\n{detail}",
            failure.kind, failure.source
        );
        if let Some(keep) = &options.keep {
            let name = format!("{}-{index}.cb", failure.kind);
            let _ = fs::write(keep.join(name), &failure.source);
        }
    }
    ExitCode::FAILURE
}

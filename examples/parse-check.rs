// SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Report whether files parse as C♭, without going through the CLI.
//!
//! `tree-sitter parse` loads the grammar from the working directory and
//! rebuilds it into a shared cache whenever `src/parser.c` looks newer, so a
//! fuzz campaign running alongside a `tree-sitter generate` would see
//! spurious failures as the parser is swapped underneath it.  This binary
//! links the parser in statically and saves a process's worth of startup per
//! file.
//!
//! ```console
//! $ cargo run --quiet --example parse-check -- a.cb b.cb
//! b.cb:3:14: parse error (ERROR)
//! ```
//!
//! Exits 0 if every file parsed cleanly and 1 otherwise.

#![feature(gen_blocks)]

use std::{env, fs, process::ExitCode};

use tree_sitter::{Node, Parser, Tree};

#[path = "walk.rs"]
mod walk;

fn parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_cflat::LANGUAGE.into())
        .expect("failed to load the C♭ grammar");
    parser
}

fn first_problem(tree: &Tree) -> Option<Node<'_>> {
    // Clean subtrees cannot hold a problem
    walk::nodes(tree, |node| node.has_error()).find(|node| node.is_error() || node.is_missing())
}

fn describe(path: &str, node: Node<'_>) -> String {
    let start = node.start_position();
    let problem = if node.is_missing() {
        "missing"
    } else {
        "parse error"
    };
    format!(
        "{path}:{}:{}: {problem} ({})",
        start.row + 1,
        start.column + 1,
        node.kind()
    )
}

fn main() -> ExitCode {
    let paths: Vec<String> = env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: parse-check <file.cb>...");
        return ExitCode::FAILURE;
    }

    let mut parser = parser();
    let outcome = paths.iter().try_fold(true, |clean, path| {
        let source =
            fs::read_to_string(path).map_err(|error| format!("could not read {path}: {error}"))?;
        let tree = parser
            .parse(&source, None)
            .expect("parser returned no tree");
        let problem = first_problem(&tree).map(|node| describe(path, node));
        problem.iter().for_each(|problem| println!("{problem}"));
        Ok::<_, String>(clean && problem.is_none())
    });

    match outcome {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problem(source: &str) -> Option<&'static str> {
        let tree = parser().parse(source, None).unwrap();
        first_problem(&tree).map(|node| node.kind())
    }

    #[test]
    fn clean_programs_have_no_problem() {
        assert_eq!(problem("fn f() -> int { return 0; }\n"), None);
        assert_eq!(problem("struct S {}\n"), None);
        // Unterminated comments are lexical errors, not parse errors
        assert_eq!(problem("fn f() -> int {}\n/* open"), None);
    }

    #[test]
    fn broken_programs_report_a_problem() {
        assert!(problem("fn f( -> int {}\n").is_some());
        assert!(problem("").is_some());
        assert!(problem("struct S { x: int, }\n").is_some());
        assert!(problem("fn f() -> int {\n  x = y = 1;\n}\n").is_some());
    }

    #[test]
    fn missing_nodes_count_as_problems() {
        // `new [int; 10]` leaves the parser wanting a `]`
        assert!(problem("fn f() -> int {\n  a = new [int; 10];\n}\n").is_some());
    }

    #[test]
    fn deep_trees_do_not_overflow_the_stack() {
        let source = format!(
            "fn f() -> int {{ return {}1{}; }}\n",
            "(".repeat(20_000),
            ")".repeat(20_000)
        );
        assert_eq!(problem(&source), None);
    }
}

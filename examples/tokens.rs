// SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Dump a C♭ file's token stream in the format of the reference compiler's
//! `-o tokens`, so the grammar's lexical layer can be diffed against the
//! course compiler's hand-written lexer:
//!
//! ```console
//! $ cargo run --quiet --example tokens -- prog.cb > ours.txt
//! $ cflat -o tokens prog.cb > theirs.txt
//! $ diff ours.txt theirs.txt
//! ```
//!
//! `scripts/cross-validate.rs` runs that over a corpus of files.

#![feature(gen_blocks)]

use std::{env, fs, process::ExitCode};

use tree_sitter::{Node, Parser, Tree};

#[path = "walk.rs"]
mod walk;

/// Map a leaf kind onto the reference lexer's `Lexeme` name.
///
/// An anonymous token's kind is its own text, so most of this table is
/// `Lexeme::text()` read backwards.
fn lexeme_name(kind: &str) -> Option<&'static str> {
    Some(match kind {
        ":" => "Colon",
        ";" => "Semicolon",
        "," => "Comma",
        "->" => "Arrow",
        "&" => "Ampersand",
        "+" => "Plus",
        "-" => "Dash",
        "*" => "Star",
        "/" => "Slash",
        "==" => "Equal",
        "!=" => "NotEq",
        "<" => "Lt",
        "<=" => "Lte",
        ">" => "Gt",
        ">=" => "Gte",
        "." => "Dot",
        "=" => "Gets",
        "(" => "OpenParen",
        ")" => "CloseParen",
        "[" => "OpenBracket",
        "]" => "CloseBracket",
        "{" => "OpenBrace",
        "}" => "CloseBrace",
        "?" => "QuestionMark",
        "and" => "And",
        "or" => "Or",
        "not" => "Not",
        "nil" => "Nil",
        "new" => "New",
        "int" => "Int",
        "if" => "If",
        "else" => "Else",
        "while" => "While",
        "break" => "Break",
        "continue" => "Continue",
        "return" => "Return",
        "let" => "Let",
        "fn" => "Fn",
        "struct" => "Struct",
        "extern" => "Extern",

        // Single-token rules collapse into a leaf named after the rule, so
        // `int` arrives as `primitive_type`.
        "primitive_type" => "Int",
        "identifier" => "Id",
        "number" => "Num",
        // Reference lexer spans the whole comment with one `Error` token
        "unterminated_comment" => "Error",

        // Including terminated comments, which the reference lexer skips
        _ => return None,
    })
}

/// Leaves inside an `ERROR` node are real tokens and are kept, but the
/// zero-width `MISSING` nodes that error recovery invents are not.
fn leaves(tree: &Tree) -> impl Iterator<Item = Node<'_>> {
    walk::nodes(tree, |_| true).filter(|node| {
        node.child_count() == 0 && !node.is_missing() && !node.byte_range().is_empty()
    })
}

fn main() -> ExitCode {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: tokens <file.cb>");
        return ExitCode::FAILURE;
    };
    let Ok(source) =
        fs::read_to_string(&path).inspect_err(|error| eprintln!("could not read {path}: {error}"))
    else {
        return ExitCode::FAILURE;
    };

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_cflat::LANGUAGE.into())
        .expect("failed to load the C♭ grammar");
    let tree = parser
        .parse(&source, None)
        .expect("parser returned no tree");

    leaves(&tree)
        .filter_map(|leaf| Some((lexeme_name(leaf.kind())?, &source[leaf.byte_range()])))
        .for_each(|(name, text)| println!("{name}: {text}"));

    ExitCode::SUCCESS
}

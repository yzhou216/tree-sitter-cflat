// SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Incremental-reparse differential tests.
//!
//! After an edit, tree-sitter reuses the untouched parts of the previous tree.
//! That reuse is sound only if the incremental reparse produces exactly the
//! tree a fresh parse would, and grammars can get this wrong in ways a
//! batch-only suite never notices, typically around `extras` and multi-line
//! tokens such as this grammar's hand-written comment patterns.  Every test
//! here performs an edit both ways and compares the two trees.

use tree_sitter::{InputEdit, Parser, Point, Tree};

fn new_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_cflat::LANGUAGE.into())
        .expect("failed to load the C♭ grammar");
    parser
}

/// Row and column of a byte offset
fn point_at(source: &str, offset: usize) -> Point {
    let preceding = &source[..offset];
    Point::new(
        preceding.matches('\n').count(),
        preceding
            .rfind('\n')
            .map_or(offset, |newline| offset - newline - 1),
    )
}

fn splice(source: &str, start: usize, end: usize, replacement: &str) -> (String, InputEdit) {
    let edited = format!("{}{replacement}{}", &source[..start], &source[end..]);
    let new_end_byte = start + replacement.len();
    let edit = InputEdit {
        start_byte: start,
        old_end_byte: end,
        new_end_byte,
        start_position: point_at(source, start),
        old_end_position: point_at(source, end),
        new_end_position: point_at(&edited, new_end_byte),
    };
    (edited, edit)
}

/// Apply `edit` to `tree` and reparse `edited` incrementally, then check the
/// result against a fresh parse.
fn reparse_and_compare(
    parser: &mut Parser,
    tree: &mut Tree,
    edited: &str,
    edit: &InputEdit,
    context: &str,
) -> Tree {
    tree.edit(edit);
    let incremental = parser
        .parse(edited, Some(tree))
        .expect("incremental parse failed");
    // Separate parser, so no state can leak between the two
    let fresh = new_parser()
        .parse(edited, None)
        .expect("fresh parse failed");
    assert_eq!(
        incremental.root_node().to_sexp(),
        fresh.root_node().to_sexp(),
        "incremental and fresh parses disagree {context}"
    );
    incremental
}

fn assert_incremental_matches_fresh(source: &str, start: usize, end: usize, replacement: &str) {
    let mut parser = new_parser();
    let mut tree = parser.parse(source, None).expect("parse failed");
    let (edited, edit) = splice(source, start, end, replacement);
    reparse_and_compare(
        &mut parser,
        &mut tree,
        &edited,
        &edit,
        &format!(
            "after editing {start}..{end} to {replacement:?}\n\
             --- source ---\n{source}\n--- edited ---\n{edited}"
        ),
    );
}

const PROGRAM: &str = "\
// Point in the plane.
struct Point {
  x: int,
  parent: &Point
}

extern print_num : (int) -> int;

fn main() -> int {
  let p: &Point, i: int;
  p = new Point;
  i = p.x;
  if i > 0 {
    i = i <= 3 ? i : 4; /* clamp */
  } else {
    i = 0;
  }
  return i;
}
";

#[test]
fn every_single_character_deletion_agrees() {
    PROGRAM.char_indices().for_each(|(offset, character)| {
        assert_incremental_matches_fresh(PROGRAM, offset, offset + character.len_utf8(), "");
    });
}

#[test]
fn every_single_character_insertion_agrees() {
    let source = "fn f() -> int { return a.*; }\n";
    // One of each lexical class that could change how a neighbour is scanned
    for text in ["x", "0", " ", "\n", "*", "/", "{", "}", ";", "-", "&", "?"] {
        (0..=source.len())
            .filter(|&offset| source.is_char_boundary(offset))
            .for_each(|offset| assert_incremental_matches_fresh(source, offset, offset, text));
    }
}

#[test]
fn closing_a_block_comment_agrees() {
    // Unterminated, the comment swallows the rest of the buffer, and typing
    // the `/` that closes it must give back everything it ate.
    let source = "fn f() -> int {}\n/* open *\nfn g() -> int {}\n";
    let slash = source.find("*\n").unwrap() + 1;
    assert_incremental_matches_fresh(source, slash, slash, "/");
}

#[test]
fn opening_a_block_comment_agrees() {
    let source = "fn f() -> int {}\nfn g() -> int {}\n";
    let start = source.find("fn g").unwrap();
    assert_incremental_matches_fresh(source, start, start, "/*");
}

#[test]
fn deleting_a_comment_terminator_agrees() {
    let source = "/* a */ fn f() -> int {}\n/* b */ fn g() -> int {}\n";
    let first_close = source.find("*/").unwrap();
    assert_incremental_matches_fresh(source, first_close, first_close + 2, "");
}

#[test]
fn removing_the_final_newline_agrees() {
    // Leaves the trailing line comment unterminated
    let source = "fn f() -> int {}\n// note\n";
    assert_incremental_matches_fresh(source, source.len() - 1, source.len(), "");
}

#[test]
fn adding_a_final_newline_agrees() {
    let source = "fn f() -> int {}\n// note";
    assert_incremental_matches_fresh(source, source.len(), source.len(), "\n");
}

#[test]
fn commenting_out_a_function_agrees() {
    let start = PROGRAM.find("extern").unwrap();
    assert_incremental_matches_fresh(PROGRAM, start, start, "// ");
}

#[test]
fn splitting_a_line_comment_with_a_newline_agrees() {
    let source = "// one long comment line\nfn f() -> int {}\n";
    let middle = source.find("long").unwrap();
    assert_incremental_matches_fresh(source, middle, middle, "\n");
}

#[test]
fn changing_an_operator_agrees() {
    let source = "fn f() -> int { return a + b * c; }\n";
    let plus = source.find('+').unwrap();
    for replacement in ["-", "*", "/", "==", "<", "and", "or", "?"] {
        assert_incremental_matches_fresh(source, plus, plus + 1, replacement);
    }
}

#[test]
fn turning_an_expression_into_a_ternary_agrees() {
    let source = "fn f() -> int { return a ? b : c; }\n";
    let end = source.find("; }").unwrap();
    assert_incremental_matches_fresh(source, end, end, " ? d : e");
}

#[test]
fn deleting_a_brace_agrees() {
    let brace = PROGRAM.find("  if i > 0 {").unwrap() + "  if i > 0 ".len();
    assert_incremental_matches_fresh(PROGRAM, brace, brace + 1, "");
}

#[test]
fn renaming_an_identifier_agrees() {
    let start = PROGRAM.find("print_num").unwrap();
    assert_incremental_matches_fresh(PROGRAM, start, start + "print_num".len(), "put");
}

#[test]
fn replacing_the_whole_buffer_agrees() {
    assert_incremental_matches_fresh(PROGRAM, 0, PROGRAM.len(), "struct S {}\n");
}

#[test]
fn emptying_the_buffer_agrees() {
    assert_incremental_matches_fresh(PROGRAM, 0, PROGRAM.len(), "");
}

#[test]
fn typing_a_program_one_character_at_a_time_agrees() {
    let target = "fn main() -> int {\n  let x: int;\n  x = f(1) ? 2 : 3;\n  return x;\n}\n";
    let mut parser = new_parser();
    let mut source = String::new();
    let mut tree = parser.parse(&source, None).expect("parse failed");

    for character in target.chars() {
        let (edited, edit) = splice(&source, source.len(), source.len(), &character.to_string());
        tree = reparse_and_compare(
            &mut parser,
            &mut tree,
            &edited,
            &edit,
            &format!("after typing up to:\n{edited}"),
        );
        source = edited;
    }

    assert!(
        !tree.root_node().has_error(),
        "finished program should parse"
    );
}

#[test]
fn deleting_a_program_one_character_at_a_time_agrees() {
    let mut parser = new_parser();
    let mut source = String::from(PROGRAM);
    let mut tree = parser.parse(&source, None).expect("parse failed");

    while let Some(last) = source.chars().next_back() {
        let start = source.len() - last.len_utf8();
        let (edited, edit) = splice(&source, start, source.len(), "");
        tree = reparse_and_compare(
            &mut parser,
            &mut tree,
            &edited,
            &edit,
            &format!("after truncating to:\n{edited}"),
        );
        source = edited;
    }
}

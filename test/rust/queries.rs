// SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the files in `queries/`.
//!
//! Query files name nodes and fields as strings, so nothing ties them to
//! `grammar.js` until something compiles them, and renaming a node would
//! otherwise break highlighting in every editor without failing a single
//! test.  These tests compile each query against the real grammar and check
//! that the captures consumers depend on actually fire.

use std::{collections::BTreeSet, fs, path::PathBuf};

use tree_sitter::{Language, Parser, Query, QueryCursor, StreamingIterator as _, Tree};

fn language() -> Language {
    tree_sitter_cflat::LANGUAGE.into()
}

fn parse(source: &str) -> Tree {
    let mut parser = Parser::new();
    parser.set_language(&language()).unwrap();
    parser.parse(source, None).unwrap()
}

fn query_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("queries")
        .join(name)
}

fn load(name: &str) -> Query {
    let source = fs::read_to_string(query_path(name))
        .unwrap_or_else(|error| panic!("could not read queries/{name}: {error}"));
    Query::new(&language(), &source)
        .unwrap_or_else(|error| panic!("queries/{name} does not compile: {error:?}"))
}

/// Capture name and captured text for every capture of `query` over `source`
fn captures<'s>(query: &Query, source: &'s str) -> Vec<(String, &'s str)> {
    let tree = parse(source);
    QueryCursor::new()
        .captures(query, tree.root_node(), source.as_bytes())
        .map_deref(|(matched, index)| {
            let capture = matched.captures[*index];
            (
                query.capture_names()[capture.index as usize].to_owned(),
                &source[capture.node.byte_range()],
            )
        })
        .collect()
}

/// Every capture name the query file `name` produces over `source`
fn capture_names(name: &str, source: &str) -> BTreeSet<String> {
    captures(&load(name), source)
        .into_iter()
        .map(|(capture, _)| capture)
        .collect()
}

/// Text of every capture of the inline query `query_source`
fn captured_text<'s>(query_source: &str, source: &'s str) -> Vec<&'s str> {
    let query = Query::new(&language(), query_source)
        .unwrap_or_else(|error| panic!("query does not compile: {error:?}"));
    captures(&query, source)
        .into_iter()
        .map(|(_, text)| text)
        .collect()
}

const PROGRAM: &str = "\
// A comment.
struct Point {
  x: int,
  parent: &Point
}

extern print_num : (int) -> int;

fn main(argc: int, argv: &[int]) -> int {
  let p: &Point, i: int;
  p = new Point;
  i = p.x;
  i = print_num(i) + 1;
  if not i >= 2 {
    i = i <= 3 ? i : 4;
  } else {
    i = nil.*;
  }
  while i > 0 { i = i - 1; }
  return i;
}
";

#[test]
fn every_query_file_compiles() {
    let compiled = fs::read_dir(query_path(""))
        .expect("queries/ is missing")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "scm"))
        .inspect(|path| {
            load(path.file_name().unwrap().to_str().unwrap());
        })
        .count();
    assert!(
        compiled >= 3,
        "expected at least 3 query files, found {compiled}"
    );
}

#[test]
fn highlights_cover_every_expected_capture() {
    let found = capture_names("highlights.scm", PROGRAM);
    for expected in [
        "comment",
        "constant.builtin",
        "function",
        "function.call",
        "keyword",
        "keyword.conditional",
        "keyword.operator",
        "keyword.repeat",
        "keyword.return",
        "number",
        "operator",
        "property",
        "punctuation.bracket",
        "punctuation.delimiter",
        "type",
        "type.builtin",
        "variable",
        "variable.parameter",
    ] {
        assert!(
            found.contains(expected),
            "highlights.scm never captured @{expected}"
        );
    }
}

#[test]
fn highlights_flag_an_unterminated_comment() {
    let found = capture_names("highlights.scm", "fn f() -> int {}\n/* never closed");
    assert!(
        found.contains("comment.error"),
        "unterminated comment not flagged"
    );
    assert!(
        found.contains("error"),
        "unterminated comment not captured as @error"
    );
}

#[test]
fn highlights_do_not_flag_a_terminated_comment() {
    let found = capture_names("highlights.scm", "fn f() -> int {}\n/* closed */\n");
    assert!(found.contains("comment"));
    assert!(
        !found.contains("comment.error"),
        "terminated comment wrongly flagged"
    );
}

#[test]
fn struct_fields_are_properties_not_parameters() {
    let fields = capture_names("highlights.scm", "struct S { a: int }\n");
    assert!(fields.contains("property"));
    assert!(!fields.contains("variable.parameter"));

    let parameters = capture_names("highlights.scm", "fn f(a: int) -> int {}\n");
    assert!(parameters.contains("variable.parameter"));
    assert!(!parameters.contains("property"));
}

#[test]
fn locals_cover_every_expected_capture() {
    let found = capture_names("locals.scm", PROGRAM);
    for expected in [
        "local.scope",
        "local.definition.function",
        "local.definition.type",
        "local.definition.parameter",
        "local.definition.var",
        "local.definition.field",
        "local.reference",
    ] {
        assert!(
            found.contains(expected),
            "locals.scm never captured @{expected}"
        );
    }
}

#[test]
fn tags_cover_every_expected_capture() {
    let found = capture_names("tags.scm", PROGRAM);
    for expected in [
        "name",
        "definition.function",
        "definition.class",
        "definition.field",
        "reference.call",
        "reference.type",
    ] {
        assert!(
            found.contains(expected),
            "tags.scm never captured @{expected}"
        );
    }
}

#[test]
fn tags_name_the_right_nodes() {
    let names: BTreeSet<&str> = captures(&load("tags.scm"), PROGRAM)
        .into_iter()
        .filter(|(capture, _)| capture == "name")
        .map(|(_, text)| text)
        .collect();
    for expected in ["Point", "print_num", "main", "x", "parent"] {
        assert!(
            names.contains(expected),
            "tags.scm never named `{expected}`; got {names:?}"
        );
    }
}

// The supertypes are hidden rules that never appear in a tree, but a query
// can still match them, which is what turns a check over every expression
// form into a single pattern.

#[test]
fn expression_supertype_matches_any_expression() {
    let found = captured_text("(_expression) @exp", "fn f() -> int { return a + 1; }\n");
    for expected in ["a + 1", "a", "1"] {
        assert!(found.contains(&expected), "got {found:?}");
    }
}

#[test]
fn statement_supertype_matches_any_statement() {
    let found = captured_text(
        "(_statement) @stmt",
        "fn f() -> int {\n  x = 1;\n  break;\n  return x;\n}\n",
    );
    for expected in ["x = 1;", "break;", "return x;"] {
        assert!(found.contains(&expected), "got {found:?}");
    }
}

#[test]
fn type_supertype_matches_any_type() {
    let found = captured_text("(_type) @ty", "fn f(a: &[int]) -> int {}\n");
    for expected in ["&[int]", "[int]", "int"] {
        assert!(found.contains(&expected), "got {found:?}");
    }
}

#[test]
fn supertypes_enable_the_lint_layer_the_readme_describes() {
    let targets = captured_text(
        "(assignment left: (_expression) @target)",
        "fn f() -> int {\n  a.b = 1;\n  x + y = 17;\n}\n",
    );
    assert_eq!(targets, ["a.b", "x + y"]);
}

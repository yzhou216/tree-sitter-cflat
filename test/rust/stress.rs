// SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pathological inputs.
//!
//! Deep nesting and very long operator chains are where an LR parser with a
//! badly specified precedence table degrades, either into quadratic time or
//! into an ERROR partway through.  These tests build inputs that are hostile
//! in each direction and check that the result is clean and complete, with
//! every node present and nested the right way, rather than a truncated tree
//! that merely happens to parse.
//!
//! Timings are too flaky to assert on, but a quadratic regression would
//! surface as the suite hanging.

#![feature(gen_blocks)]

use std::iter;

use tree_sitter::{Node, Parser, Tree};

const DEEP: usize = 2000;
const LONG: usize = 20_000;

fn parse(source: &str) -> Tree {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_cflat::LANGUAGE.into())
        .expect("failed to load the C♭ grammar");
    parser.parse(source, None).expect("parser returned no tree")
}

fn parse_clean(source: &str, what: &str) -> Tree {
    let tree = parse(source);
    assert!(
        !tree.root_node().has_error(),
        "{what} did not parse cleanly"
    );
    tree
}

/// Every node of `tree`, parents before children.
///
/// These trees are tens of thousands of levels deep, so a recursive walk would
/// overflow the test thread's stack long before the parser showed any strain.
fn nodes(tree: &Tree) -> impl Iterator<Item = Node<'_>> {
    gen move {
        let mut cursor = tree.walk();
        loop {
            yield cursor.node();
            if cursor.goto_first_child() {
                continue;
            }
            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() {
                    return;
                }
            }
        }
    }
}

fn count_kind(tree: &Tree, kind: &str) -> usize {
    nodes(tree).filter(|node| node.kind() == kind).count()
}

/// Expression in `fn f() -> int { return <exp>; }`
fn returned(tree: &Tree) -> Node<'_> {
    tree.root_node()
        .named_child(0)
        .and_then(|function| function.child_by_field_name("body"))
        .and_then(|body| body.named_child(0))
        .and_then(|statement| statement.child_by_field_name("value"))
        .expect("no returned expression")
}

/// Length of the run of `kind` nodes reached from `node` through `field`
fn chain(node: Node<'_>, field: &str, kind: &str) -> usize {
    iter::successors(Some(node), |node| node.child_by_field_name(field))
        .take_while(|node| node.kind() == kind)
        .count()
}

fn returning(expression: &str) -> String {
    format!("fn f() -> int {{ return {expression}; }}\n")
}

#[test]
fn deeply_nested_parentheses() {
    let source = returning(&format!("{}1{}", "(".repeat(DEEP), ")".repeat(DEEP)));
    let tree = parse_clean(&source, "deeply nested parentheses");
    assert_eq!(
        count_kind(&tree, "parenthesized_expression"),
        DEEP,
        "some parentheses were dropped"
    );
}

#[test]
fn deeply_nested_array_types() {
    let source = format!(
        "fn f(x: {}int{}) -> int {{}}\n",
        "[".repeat(DEEP),
        "]".repeat(DEEP)
    );
    let tree = parse_clean(&source, "deeply nested array types");
    assert_eq!(count_kind(&tree, "array_type"), DEEP);
}

#[test]
fn deeply_nested_pointer_types() {
    let source = format!("fn f(x: {}int) -> int {{}}\n", "&".repeat(DEEP));
    let tree = parse_clean(&source, "deeply nested pointer types");
    assert_eq!(count_kind(&tree, "pointer_type"), DEEP);
}

#[test]
fn deeply_stacked_unary_operators() {
    let source = returning(&format!("{}a", "not ".repeat(DEEP)));
    let tree = parse_clean(&source, "stacked unary operators");
    assert_eq!(count_kind(&tree, "unary_expression"), DEEP);
    assert_eq!(chain(returned(&tree), "operand", "unary_expression"), DEEP);
}

#[test]
fn very_long_left_associative_chain() {
    let source = returning(&vec!["1"; LONG].join(" + "));
    let tree = parse_clean(&source, "long additive chain");
    assert_eq!(count_kind(&tree, "binary_expression"), LONG - 1);
    assert_eq!(
        chain(returned(&tree), "left", "binary_expression"),
        LONG - 1
    );
}

#[test]
fn very_long_right_associative_chain() {
    let source = returning(&vec!["a"; LONG].join(" and "));
    let tree = parse_clean(&source, "long logical chain");
    assert_eq!(count_kind(&tree, "binary_expression"), LONG - 1);
    assert_eq!(
        chain(returned(&tree), "right", "binary_expression"),
        LONG - 1
    );
}

#[test]
fn very_long_ternary_chain() {
    let source = returning(&format!("a{}", " ? b : c".repeat(LONG)));
    let tree = parse_clean(&source, "long ternary chain");
    assert_eq!(count_kind(&tree, "ternary_expression"), LONG);
    assert_eq!(
        chain(returned(&tree), "condition", "ternary_expression"),
        LONG,
        "the ternary chain is not left-nested"
    );
}

#[test]
fn very_long_postfix_chain() {
    let source = returning(&format!("a{}", ".b".repeat(LONG)));
    let tree = parse_clean(&source, "long field-access chain");
    assert_eq!(count_kind(&tree, "field_access"), LONG);
    assert_eq!(chain(returned(&tree), "object", "field_access"), LONG);
}

#[test]
fn very_long_call_chain() {
    let source = returning(&format!("a{}", "()".repeat(LONG)));
    let tree = parse_clean(&source, "long call chain");
    assert_eq!(count_kind(&tree, "call_expression"), LONG);
    assert_eq!(chain(returned(&tree), "function", "call_expression"), LONG);
}

#[test]
fn many_top_level_items() {
    let items: String = (0..5_000)
        .map(|index| format!("fn f{index}() -> int {{ return {index}; }}\n"))
        .collect();
    let tree = parse_clean(&items, "many top-level items");
    assert_eq!(count_kind(&tree, "function_definition"), 5_000);
}

#[test]
fn a_very_long_comment() {
    let source = format!("/*{}*/\nfn f() -> int {{}}\n", "x".repeat(1_000_000));
    let tree = parse_clean(&source, "a one-megabyte comment");
    assert_eq!(count_kind(&tree, "comment"), 1);
}

#[test]
fn many_comments() {
    let comments: String = (0..20_000).map(|index| format!("// {index}\n")).collect();
    let source = format!("{comments}fn f() -> int {{}}\n");
    let tree = parse_clean(&source, "many comments");
    assert_eq!(count_kind(&tree, "comment"), 20_000);
}

#[test]
fn error_recovery_terminates_on_pathological_input() {
    // None of these are C♭, so the parser only has to finish with a tree
    for source in [
        "{".repeat(20_000),
        "}".repeat(20_000),
        "(".repeat(20_000),
        "fn ".repeat(20_000),
        "let ".repeat(20_000),
        ".".repeat(20_000),
        "?".repeat(20_000),
        "/*".repeat(20_000),
        "a".repeat(200_000),
        "1".repeat(200_000),
    ] {
        // Deliberately not `to_sexp()`, which recurses in C and would abort on
        // these deep trees for reasons unrelated to the grammar.
        assert!(!parse(&source).root_node().byte_range().is_empty());
    }
}

// SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for the C♭ grammar.
//!
//! These complement `tree-sitter test`, which cannot express two things.
//! Corpus expectations print only named nodes, so `a + b` and `a - b` have
//! identical expected trees and no `field(...)` is ever checked.  A corpus
//! entry's source is also always newline-terminated before its `---`
//! divider, which puts a line comment on a final line with no newline out of
//! reach.

use tree_sitter::{Node, Parser, Tree};

fn parse(source: &str) -> Tree {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_cflat::LANGUAGE.into())
        .expect("failed to load the C♭ grammar");
    parser.parse(source, None).expect("parser returned no tree")
}

fn parse_ok(source: &str) -> Tree {
    let tree = parse(source);
    assert!(
        !tree.root_node().has_error(),
        "expected a clean parse, got: {}",
        tree.root_node().to_sexp()
    );
    tree
}

/// Wrap an expression in the smallest legal program that contains it
fn expression(source: &str) -> (Tree, String) {
    let program = format!("fn f() -> int {{\n  return {source};\n}}\n");
    (parse_ok(&program), program)
}

/// Expression returned by the first function's first statement
fn returned(tree: &Tree) -> Node<'_> {
    let function = tree.root_node().named_child(0).unwrap();
    assert_eq!(function.kind(), "function_definition");
    let statement = function
        .child_by_field_name("body")
        .and_then(|body| body.named_child(0))
        .unwrap();
    assert_eq!(statement.kind(), "return_statement");
    statement.child_by_field_name("value").unwrap()
}

fn field<'t>(node: Node<'t>, name: &str) -> Node<'t> {
    node.child_by_field_name(name)
        .unwrap_or_else(|| panic!("node {} has no field `{name}`", node.kind()))
}

fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

/// Kind and text of every comment directly under the root
fn extras_of(source: &str) -> Vec<String> {
    let tree = parse(source);
    let mut cursor = tree.root_node().walk();
    tree.root_node()
        .named_children(&mut cursor)
        .filter(|node| matches!(node.kind(), "comment" | "unterminated_comment"))
        .map(|node| format!("{}:{}", node.kind(), text(node, source)))
        .collect()
}

#[test]
fn ternary_is_left_associative() {
    let (tree, program) = expression("a ? b : c ? d : e");
    let outer = returned(&tree);

    assert_eq!(outer.kind(), "ternary_expression");
    let condition = field(outer, "condition");
    assert_eq!(
        condition.kind(),
        "ternary_expression",
        "the nested ternary must sit in the CONDITION position"
    );
    assert_eq!(text(condition, &program), "a ? b : c");
    assert_eq!(text(field(outer, "consequence"), &program), "d");
    assert_eq!(text(field(outer, "alternative"), &program), "e");
}

#[test]
fn ternary_nests_in_the_consequence_position() {
    let (tree, program) = expression("a ? b ? c : d : e");
    let outer = returned(&tree);

    assert_eq!(text(field(outer, "condition"), &program), "a");
    let consequence = field(outer, "consequence");
    assert_eq!(consequence.kind(), "ternary_expression");
    assert_eq!(text(consequence, &program), "b ? c : d");
    assert_eq!(text(field(outer, "alternative"), &program), "e");
}

#[test]
fn logical_operators_are_right_associative_and_share_a_level() {
    let (tree, program) = expression("a and b or c");
    let outer = returned(&tree);

    assert_eq!(outer.kind(), "binary_expression");
    assert_eq!(text(field(outer, "operator"), &program), "and");
    assert_eq!(text(field(outer, "left"), &program), "a");

    let right = field(outer, "right");
    assert_eq!(right.kind(), "binary_expression");
    assert_eq!(text(field(right, "operator"), &program), "or");
    assert_eq!(text(right, &program), "b or c");
}

#[test]
fn arithmetic_is_left_associative() {
    let (tree, program) = expression("a - b - c");
    let outer = returned(&tree);

    assert_eq!(text(field(outer, "left"), &program), "a - b");
    assert_eq!(text(field(outer, "right"), &program), "c");
}

#[test]
fn unary_is_right_associative() {
    let (tree, program) = expression("not not a");
    let outer = returned(&tree);

    assert_eq!(outer.kind(), "unary_expression");
    assert_eq!(text(field(outer, "operator"), &program), "not");
    let inner = field(outer, "operand");
    assert_eq!(inner.kind(), "unary_expression");
    assert_eq!(text(field(inner, "operand"), &program), "a");
}

#[test]
fn precedence_ladder() {
    let (tree, program) = expression("p ? not a * b + c < d and e : f");
    let ternary = returned(&tree);
    assert_eq!(ternary.kind(), "ternary_expression");

    let logical = field(ternary, "consequence");
    assert_eq!(text(field(logical, "operator"), &program), "and");
    let relational = field(logical, "left");
    assert_eq!(text(field(relational, "operator"), &program), "<");
    let additive = field(relational, "left");
    assert_eq!(text(field(additive, "operator"), &program), "+");
    let multiplicative = field(additive, "left");
    assert_eq!(text(field(multiplicative, "operator"), &program), "*");
    assert_eq!(text(field(multiplicative, "left"), &program), "not a");
}

#[test]
fn every_binary_operator_is_recorded_in_the_operator_field() {
    for operator in [
        "+", "-", "*", "/", "==", "!=", "<", "<=", ">", ">=", "and", "or",
    ] {
        let (tree, program) = expression(&format!("a {operator} b"));
        let node = returned(&tree);
        assert_eq!(node.kind(), "binary_expression", "for `{operator}`");
        assert_eq!(text(field(node, "operator"), &program), operator);
    }
}

#[test]
fn postfix_operators_chain_left_to_right() {
    let (tree, program) = expression("b.c[0].*(d).e");
    let node = returned(&tree);

    assert_eq!(node.kind(), "field_access");
    assert_eq!(text(field(node, "field"), &program), "e");

    let call = field(node, "object");
    assert_eq!(call.kind(), "call_expression");

    let deref = field(call, "function");
    assert_eq!(deref.kind(), "pointer_access");

    let index = field(deref, "object");
    assert_eq!(index.kind(), "index_expression");
    assert_eq!(text(field(index, "index"), &program), "0");

    let inner = field(index, "array");
    assert_eq!(inner.kind(), "field_access");
    assert_eq!(text(field(inner, "object"), &program), "b");
    assert_eq!(text(field(inner, "field"), &program), "c");
}

#[test]
fn allocation_forms_are_distinct() {
    let (tree, _) = expression("new Point");
    let node = returned(&tree);
    assert_eq!(node.kind(), "allocation");
    assert_eq!(field(node, "type").kind(), "struct_type");

    let (tree, _) = expression("[int; 10]");
    let node = returned(&tree);
    assert_eq!(node.kind(), "array_allocation");
    assert_eq!(field(node, "type").kind(), "primitive_type");
    assert_eq!(field(node, "size").kind(), "number");
}

#[test]
fn line_comment_is_terminated_by_its_newline() {
    assert_eq!(
        extras_of("fn f() -> int {}\n// done\n"),
        ["comment:// done\n"]
    );
}

#[test]
fn line_comment_on_the_final_line_without_a_newline_is_unterminated() {
    assert_eq!(
        extras_of("fn f() -> int {}\n// done"),
        ["unterminated_comment:// done"]
    );
}

#[test]
fn block_comment_is_non_greedy() {
    assert_eq!(
        extras_of("/* one */ fn f() -> int {} /* two */"),
        ["comment:/* one */", "comment:/* two */"]
    );
}

#[test]
fn block_comment_tolerates_stars_that_do_not_close_it() {
    assert_eq!(
        extras_of("/* a * b ** c * / d */ fn f() -> int {}"),
        ["comment:/* a * b ** c * / d */"]
    );
}

#[test]
fn slash_immediately_after_the_opener_does_not_close_the_comment() {
    assert_eq!(
        extras_of("/*/ still open */ fn f() -> int {}"),
        ["comment:/*/ still open */"]
    );
    assert_eq!(extras_of("/**/ fn f() -> int {}"), ["comment:/**/"]);
    assert_eq!(extras_of("/***/ fn f() -> int {}"), ["comment:/***/"]);
}

#[test]
fn unterminated_block_comment_runs_to_end_of_input() {
    assert_eq!(
        extras_of("fn f() -> int {}\n/* never closed * / here"),
        ["unterminated_comment:/* never closed * / here"]
    );
}

#[test]
fn keyword_extraction_does_not_split_identifiers() {
    let tree = parse_ok("fn newton(iffy: int) -> int {\n  return iffy;\n}\n");
    let function = tree.root_node().named_child(0).unwrap();
    assert_eq!(function.kind(), "function_definition");
    assert_eq!(field(function, "name").kind(), "identifier");
}

#[test]
fn over_generation_is_accepted_because_it_is_a_semantic_rule() {
    for source in [
        "fn f() -> int {\n  x;\n}\n",
        "fn f() -> int {\n  x + y = 17;\n}\n",
        "fn f() -> int {}\nfn f() -> int {}\n",
        "struct S {\n  x: int,\n  x: &Point\n}\n",
    ] {
        assert!(
            !parse(source).root_node().has_error(),
            "expected the grammar to accept:\n{source}"
        );
    }
}

#[test]
fn rejects_forms_outside_the_reference_grammar() {
    for source in [
        // Trailing comma, which LIST does not allow
        "struct Point {\n  x: int,\n}\n",
        "fn f(x: int,) -> int {}\n",
        // Top-level `let`, which is function-local
        "let f : (int) -> int;\n",
        // `new [type; exp]`, which does not exist
        "fn f() -> int {\n  a = new [int; 10];\n}\n",
        // Nested and chained assignment, since assignment is a statement
        "fn f() -> int {\n  foo(x = 1);\n}\n",
        "fn f() -> int {\n  x = y = 1;\n}\n",
        // `let` after a statement
        "fn f() -> int {\n  x = 0;\n  let y: int;\n}\n",
        // No items at all
        "",
        "// just a comment\n",
    ] {
        assert!(
            parse(source).root_node().has_error(),
            "expected the grammar to reject:\n{source}"
        );
    }
}

// Whitespace is exactly ` `, `\t`, `\n` and `\r`, not everything `\s` matches.
// The cases below need literal control characters, so they cannot live in a
// corpus file.

#[test]
fn crlf_line_endings_are_whitespace() {
    parse_ok("fn f() -> int {\r\n  return 0;\r\n}\r\n");
}

#[test]
fn lone_carriage_returns_are_whitespace() {
    parse_ok("fn f() -> int {\r  return 0;\r}\r");
}

#[test]
fn tabs_are_whitespace() {
    parse_ok("fn\tf()\t->\tint\t{\n\treturn\t0;\n}\n");
}

#[test]
fn vertical_tab_and_form_feed_are_not_whitespace() {
    for separator in ["\x0b", "\x0c"] {
        let source = format!("fn{separator}f() -> int {{}}\n");
        assert!(
            parse(&source).root_node().has_error(),
            "{separator:?} should not be accepted as whitespace"
        );
    }
}

#[test]
fn a_crlf_terminates_a_line_comment() {
    assert_eq!(
        extras_of("fn f() -> int {}\r\n// done\r\n"),
        ["comment:// done\r\n"]
    );
    parse_ok("// note\r\nfn f() -> int { return 0; }\r\n");
}

#[test]
fn a_lone_carriage_return_does_not_terminate_a_line_comment() {
    // Reference lexer likewise emits one error token over the whole span
    let source = "// note\rfn f() -> int { return 0; }\r";
    let tree = parse(source);
    let mut cursor = tree.root_node().walk();
    let children: Vec<(&str, &str)> = tree
        .root_node()
        .named_children(&mut cursor)
        .map(|node| (node.kind(), text(node, source)))
        .collect();
    assert_eq!(
        children,
        [("unterminated_comment", source)],
        "the comment should have swallowed the whole file"
    );
}

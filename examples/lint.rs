// SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Enforce the C♭ rules the grammar deliberately cannot.
//!
//! The reference CFG in `cflat.cfg` over-generates, and the real compiler
//! rejects the surplus while building the AST rather than while parsing.  A
//! tree-sitter grammar is syntax only, so those checks belong here; see the
//! README section "What this grammar deliberately does not enforce".
//!
//! ```console
//! $ cargo run --quiet --example lint -- prog.cb
//! prog.cb:3:3: expression statement is not a function call
//! prog.cb:4:3: assignment target is not a place (a variable, `.field`, `.*` or `[index]`)
//! ```
//!
//! Exits 1 if anything was reported and 0 if every file is clean.

#![feature(gen_blocks)]

use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    env, fmt, fs,
    process::ExitCode,
};

use tree_sitter::{Node, Parser, Tree};

#[path = "walk.rs"]
mod walk;

#[derive(Debug, PartialEq, Eq)]
struct Diagnostic {
    row: usize,
    column: usize,
    message: String,
}

impl Diagnostic {
    fn at(node: Node<'_>, message: impl Into<String>) -> Self {
        let start = node.start_position();
        Self {
            row: start.row + 1,
            column: start.column + 1,
            message: message.into(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}: {}", self.row, self.column, self.message)
    }
}

/// Whether `node` may appear on the left of an assignment.
///
/// The course notes list only `exp.*`, `exp[exp]` and `exp.id`, but a bare
/// identifier plainly belongs too: the same page's examples include `x = 0;`,
/// and without it no local could ever be assigned to.  A parenthesized place
/// is rejected, matching the list literally.
fn is_place(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "identifier" | "field_access" | "pointer_access" | "index_expression"
    )
}

fn parse(source: &str) -> Tree {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_cflat::LANGUAGE.into())
        .expect("failed to load the C♭ grammar");
    parser.parse(source, None).expect("parser returned no tree")
}

fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

fn declarations(container: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = container.walk();
    container
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "declaration")
        .collect()
}

/// `let`s are function-wide, so locals collide across separate declarations
/// and are collected per body rather than per `let`.
fn locals(body: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter(|statement| statement.kind() == "let_declaration")
        .flat_map(declarations)
        .collect()
}

/// Report names declared more than once with *different* types, since an
/// exactly repeated declaration is legal and collapses to one.
fn duplicate_declarations<'t>(
    declarations: impl IntoIterator<Item = Node<'t>>,
    source: &str,
    what: &str,
) -> Vec<Diagnostic> {
    let mut seen: HashMap<&str, &str> = HashMap::new();
    declarations
        .into_iter()
        .filter_map(|declaration| {
            let name = declaration.child_by_field_name("name")?;
            let kind = declaration.child_by_field_name("type")?;
            Some((name, text(name, source), text(kind, source)))
        })
        .filter_map(|(node, name, kind)| match seen.entry(name) {
            Entry::Vacant(slot) => {
                slot.insert(kind);
                None
            }
            Entry::Occupied(slot) if *slot.get() == kind => None,
            Entry::Occupied(slot) => Some(Diagnostic::at(
                node,
                format!(
                    "duplicate {what} `{name}`, already declared with type `{}`",
                    slot.get()
                ),
            )),
        })
        .collect()
}

fn non_call_statement(node: Node<'_>) -> Option<Diagnostic> {
    let expression = node.child_by_field_name("expression")?;
    (expression.kind() != "call_expression")
        .then(|| Diagnostic::at(expression, "expression statement is not a function call"))
}

fn non_place_target(node: Node<'_>) -> Option<Diagnostic> {
    let left = node.child_by_field_name("left")?;
    (!is_place(left)).then(|| {
        Diagnostic::at(
            left,
            "assignment target is not a place (a variable, `.field`, `.*` or `[index]`)",
        )
    })
}

fn duplicate_top_level_names(root: Node<'_>, source: &str) -> Vec<Diagnostic> {
    let mut cursor = root.walk();
    let mut seen = HashSet::new();
    root.named_children(&mut cursor)
        .filter_map(|item| item.child_by_field_name("name"))
        .filter(|name| !seen.insert(text(*name, source)))
        .map(|name| {
            Diagnostic::at(
                name,
                format!("duplicate top-level name `{}`", text(name, source)),
            )
        })
        .collect()
}

fn lint(tree: &Tree, source: &str) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = duplicate_top_level_names(tree.root_node(), source)
        .into_iter()
        .chain(
            walk::nodes(tree, |_| true).flat_map(|node| match node.kind() {
                "expression_statement" => Vec::from_iter(non_call_statement(node)),
                "assignment" => Vec::from_iter(non_place_target(node)),
                "field_declaration_list" => {
                    duplicate_declarations(declarations(node), source, "field")
                }
                "parameter_list" => duplicate_declarations(declarations(node), source, "parameter"),
                "function_body" => duplicate_declarations(locals(node), source, "local"),
                _ => Vec::new(),
            }),
        )
        .collect();
    diagnostics.sort_by_key(|diagnostic| (diagnostic.row, diagnostic.column));
    diagnostics
}

fn main() -> ExitCode {
    let paths: Vec<String> = env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: lint <file.cb>...");
        return ExitCode::FAILURE;
    }

    let outcome = paths.iter().try_fold(true, |clean, path| {
        let source =
            fs::read_to_string(path).map_err(|error| format!("could not read {path}: {error}"))?;
        let tree = parse(&source);
        let reports: Vec<String> = if tree.root_node().has_error() {
            vec![format!("{path}: does not parse; fix the syntax first")]
        } else {
            lint(&tree, &source)
                .iter()
                .map(|diagnostic| format!("{path}:{diagnostic}"))
                .collect()
        };
        reports.iter().for_each(|report| println!("{report}"));
        Ok::<_, String>(clean && reports.is_empty())
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

    fn diagnostics(source: &str) -> Vec<Diagnostic> {
        lint(&parse(source), source)
    }

    fn messages(source: &str) -> Vec<String> {
        diagnostics(source)
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect()
    }

    #[test]
    fn a_clean_program_reports_nothing() {
        let source = "struct Point {\n  x: int\n}\n\
                      fn f(a: int) -> int {\n  let b: int;\n  b.* = a;\n  g(a);\n  return a;\n}\n";
        assert!(messages(source).is_empty());
    }

    #[test]
    fn expression_statements_must_be_calls() {
        assert_eq!(
            messages("fn f() -> int {\n  x;\n}\n"),
            ["expression statement is not a function call"]
        );
        assert!(messages("fn f() -> int {\n  g();\n}\n").is_empty());
        assert!(messages("fn f() -> int {\n  a.b();\n}\n").is_empty());
    }

    #[test]
    fn assignment_targets_must_be_places() {
        for target in ["a", "a.b", "a.*", "a[0]", "a.b[0].*.c"] {
            let source = format!("fn f() -> int {{\n  {target} = 1;\n}}\n");
            assert!(messages(&source).is_empty(), "{target} should be a place");
        }
        for target in ["x + y", "17", "f()", "nil", "new Point", "(a.b)"] {
            let source = format!("fn f() -> int {{\n  {target} = 1;\n}}\n");
            assert_eq!(messages(&source).len(), 1, "{target} should not be a place");
        }
    }

    #[test]
    fn top_level_names_must_be_unique() {
        let messages = messages("fn f() -> int {}\nstruct f {}\nextern f : () -> int;\n");
        assert_eq!(messages.len(), 2, "got {messages:?}");
        assert!(messages[0].contains("duplicate top-level name `f`"));
    }

    #[test]
    fn duplicate_fields_with_different_types_are_errors() {
        let messages = messages("struct S {\n  x: int,\n  x: &Point\n}\n");
        assert_eq!(messages.len(), 1, "got {messages:?}");
        assert!(messages[0].contains("duplicate field `x`"));
    }

    #[test]
    fn identical_repeated_declarations_collapse() {
        assert!(messages("struct S {\n  x: int,\n  x: int\n}\n").is_empty());
        assert!(messages("fn f(a: int, a: int) -> int {}\n").is_empty());
        assert!(messages("fn f() -> int {\n  let a: int, a: int;\n}\n").is_empty());
    }

    #[test]
    fn duplicate_parameters_are_errors() {
        let messages = messages("fn f(a: int, a: &Point) -> int {}\n");
        assert_eq!(messages.len(), 1, "got {messages:?}");
        assert!(messages[0].contains("duplicate parameter `a`"));
    }

    #[test]
    fn locals_collide_across_separate_let_declarations() {
        let messages = messages("fn f() -> int {\n  let a: int;\n  let a: &Point;\n}\n");
        assert_eq!(messages.len(), 1, "got {messages:?}");
        assert!(messages[0].contains("duplicate local `a`"));
    }

    #[test]
    fn diagnostics_are_ordered_by_position() {
        let diagnostics = diagnostics("fn f() -> int {\n  x;\n  y + z = 1;\n}\n");
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].row < diagnostics[1].row);
    }

    #[test]
    fn diagnostics_render_as_row_column_message() {
        assert_eq!(
            diagnostics("fn f() -> int {\n  x;\n}\n")[0].to_string(),
            "2:3: expression statement is not a function call"
        );
    }

    #[test]
    fn the_over_generation_corpus_is_exactly_what_this_catches() {
        for source in [
            "fn f() -> int {\n  x;\n}\n",
            "fn f() -> int {\n  x + y = 17;\n}\n",
            "fn f() -> int {}\nfn f() -> int {}\n",
            "struct S {\n  x: int,\n  x: &Point\n}\n",
        ] {
            assert!(!messages(source).is_empty(), "the linter missed:\n{source}");
        }
    }
}

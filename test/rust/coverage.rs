// SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Coverage checks over the corpus.
//!
//! A grammar can grow a node or a field that no test ever produces while the
//! corpus suite stays green.  These tests ask the compiled grammar what it can
//! produce, through the `Language` reflection API that `src/node-types.json`
//! is generated from, and assert the corpus reaches all of it.

#![feature(gen_blocks)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fs, iter,
    path::PathBuf,
    sync::LazyLock,
};

use tree_sitter::{Language, Node, Parser, Tree};

fn language() -> Language {
    tree_sitter_cflat::LANGUAGE.into()
}

/// Every named node kind that can appear in a tree.
///
/// Hidden kinds such as the supertypes never appear, so they cannot be
/// covered.  `ERROR` is left out as well: `rejections.txt` produces it on
/// purpose, but it is not part of the language.
fn declared_node_kinds() -> BTreeSet<&'static str> {
    let language = language();
    (0..language.node_kind_count() as u16)
        .filter(|&id| language.node_kind_is_named(id) && language.node_kind_is_visible(id))
        .filter_map(|id| language.node_kind_for_id(id))
        .filter(|kind| !matches!(*kind, "ERROR" | "_ERROR"))
        .collect()
}

fn declared_fields() -> BTreeSet<&'static str> {
    let language = language();
    // Field ids start at 1
    (1..=language.field_count() as u16)
        .filter_map(|id| language.field_name_for_id(id))
        .collect()
}

fn is_rule(line: &str, character: char) -> bool {
    let line = line.trim_end();
    line.len() >= 10 && line.chars().all(|found| found == character)
}

/// Split a corpus file into the source half of each of its tests.
///
/// Each test is a `====` header block holding its name and any `:attributes`,
/// then the source, then a `----` divider, then the expected tree.
fn split_tests(text: &str) -> Vec<String> {
    let mut lines = text.lines();
    iter::from_fn(|| {
        // Past the header's opening rule, then past its closing one
        lines.find(|line| is_rule(line, '='))?;
        lines.find(|line| is_rule(line, '='))?;
        let body: Vec<&str> = lines
            .by_ref()
            .take_while(|line| !is_rule(line, '-'))
            .collect();
        Some(body.join("\n").trim_matches('\n').to_owned())
    })
    .filter(|source| !source.is_empty())
    .map(|source| source + "\n")
    .collect()
}

/// Source of every test in `test/corpus/*.txt`, tagged with its file's stem
static CORPUS: LazyLock<Vec<(String, String)>> = LazyLock::new(|| {
    let mut files: Vec<PathBuf> =
        fs::read_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test/corpus"))
            .expect("test/corpus is missing")
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "txt"))
            .collect();
    files.sort();

    files
        .iter()
        .flat_map(|file| {
            let stem = file.file_stem().unwrap().to_string_lossy().into_owned();
            split_tests(&fs::read_to_string(file).unwrap())
                .into_iter()
                .map(move |source| (stem.clone(), source))
        })
        .collect()
});

fn parse(source: &str) -> Tree {
    let mut parser = Parser::new();
    parser
        .set_language(&language())
        .expect("failed to load the C♭ grammar");
    parser.parse(source, None).expect("parser returned no tree")
}

/// Every node of `tree`, parents first, with the field it fills in its parent
fn nodes(tree: &Tree) -> impl Iterator<Item = (Node<'_>, Option<&'static str>)> {
    gen move {
        let mut cursor = tree.walk();
        loop {
            yield (cursor.node(), cursor.field_name());
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

/// Named node kinds and field names that the whole corpus produces
static CORPUS_COVERAGE: LazyLock<(BTreeSet<&str>, BTreeSet<&str>)> = LazyLock::new(|| {
    let trees: Vec<Tree> = CORPUS.iter().map(|(_, source)| parse(source)).collect();
    let visits = || trees.iter().flat_map(nodes);
    (
        visits()
            .filter(|(node, _)| node.is_named())
            .map(|(node, _)| node.kind())
            .collect(),
        visits().filter_map(|(_, field)| field).collect(),
    )
});

#[test]
fn corpus_exercises_every_named_node_type() {
    let declared = declared_node_kinds();
    let (covered, _) = &*CORPUS_COVERAGE;

    assert!(
        declared.len() >= 30,
        "the grammar declares only {} visible named node kinds, which is \
         suspiciously few: {declared:?}",
        declared.len()
    );
    let missing: Vec<_> = declared.difference(covered).collect();
    assert!(
        missing.is_empty(),
        "these node types are never produced by any corpus test: {missing:?}"
    );
}

#[test]
fn corpus_exercises_every_field_name() {
    let declared = declared_fields();
    let (_, covered) = &*CORPUS_COVERAGE;

    assert!(
        declared.len() >= 10,
        "the grammar declares only {} fields: {declared:?}",
        declared.len()
    );
    let missing: Vec<_> = declared.difference(covered).collect();
    assert!(
        missing.is_empty(),
        "these fields are never populated by any corpus test: {missing:?}"
    );
}

#[test]
fn every_non_error_corpus_test_parses_cleanly() {
    let tests: Vec<_> = CORPUS
        .iter()
        .filter(|(file, _)| file != "rejections")
        .collect();
    assert!(tests.len() > 50, "only found {} corpus tests", tests.len());

    for (file, source) in tests {
        let tree = parse(source);
        assert!(
            !tree.root_node().has_error(),
            "corpus test in {file}.txt does not parse:\n{source}\n{}",
            tree.root_node().to_sexp()
        );
    }
}

#[test]
fn every_rejection_corpus_test_really_fails() {
    let tests: Vec<_> = CORPUS
        .iter()
        .filter(|(file, _)| file == "rejections")
        .collect();
    assert!(
        tests.len() >= 10,
        "only found {} rejection tests",
        tests.len()
    );

    for (_, source) in tests {
        assert!(
            parse(source).root_node().has_error(),
            "a rejections.txt entry parses cleanly but should not:\n{source}"
        );
    }
}

#[test]
fn the_grammar_declares_the_nodes_we_document() {
    let declared = declared_node_kinds();
    for expected in [
        "source_file",
        "struct_declaration",
        "extern_declaration",
        "function_definition",
        "let_declaration",
        "assignment",
        "if_statement",
        "while_statement",
        "return_statement",
        "binary_expression",
        "unary_expression",
        "ternary_expression",
        "call_expression",
        "field_access",
        "pointer_access",
        "index_expression",
        "array_allocation",
        "allocation",
        "function_type",
        "array_type",
        "pointer_type",
        "comment",
        "unterminated_comment",
    ] {
        assert!(
            declared.contains(expected),
            "the grammar has no {expected} node"
        );
    }

    let fields = declared_fields();
    for expected in [
        "name",
        "type",
        "value",
        "condition",
        "consequence",
        "alternative",
        "operator",
        "left",
        "right",
        "operand",
        "object",
        "field",
        "function",
        "arguments",
        "array",
        "index",
        "size",
        "parameters",
        "return_type",
        "body",
        "element",
        "pointee",
    ] {
        assert!(
            fields.contains(expected),
            "the grammar has no `{expected}` field"
        );
    }
}

/// Inventory rather than assertion, shown with `-- --nocapture`
#[test]
fn report_coverage() {
    let declared_kinds = declared_node_kinds();
    let declared_field_names = declared_fields();
    let (covered_kinds, covered_fields) = &*CORPUS_COVERAGE;

    let per_file = CORPUS
        .iter()
        .fold(BTreeMap::<&str, usize>::new(), |mut counts, (file, _)| {
            *counts.entry(file).or_default() += 1;
            counts
        });

    println!("corpus tests per file:");
    per_file
        .iter()
        .for_each(|(file, count)| println!("  {file:<20} {count}"));
    println!("  {:<20} {}", "TOTAL", per_file.values().sum::<usize>());
    println!(
        "node kinds: {}/{} covered",
        covered_kinds.intersection(&declared_kinds).count(),
        declared_kinds.len()
    );
    println!(
        "fields:     {}/{} covered",
        covered_fields.intersection(&declared_field_names).count(),
        declared_field_names.len()
    );
}

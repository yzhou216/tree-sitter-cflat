// SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
// SPDX-License-Identifier: GPL-3.0-or-later

//! C♭ grammar for the [tree-sitter] parsing library.
//!
//! Load [`LANGUAGE`] into a tree-sitter [`Parser`] to parse C♭ source:
//!
//! ```
//! let code = "fn square(x: int) -> int {\n  return x * x;\n}\n";
//! let mut parser = tree_sitter::Parser::new();
//! parser
//!     .set_language(&tree_sitter_cflat::LANGUAGE.into())
//!     .expect("Error loading C♭ parser");
//! let tree = parser.parse(code, None).unwrap();
//! assert!(!tree.root_node().has_error());
//! ```
//!
//! [`Parser`]: https://docs.rs/tree-sitter/0.26/tree_sitter/struct.Parser.html
//! [tree-sitter]: https://tree-sitter.github.io/

use tree_sitter_language::LanguageFn;

unsafe extern "C" {
    fn tree_sitter_cflat() -> *const ();
}

/// Tree-sitter [`LanguageFn`] for C♭
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_cflat) };

/// Contents of [`node-types.json`], which describes every node the grammar
/// can produce
///
/// [`node-types.json`]: https://tree-sitter.github.io/tree-sitter/using-parsers/6-static-node-types
pub const NODE_TYPES: &str = include_str!("../../src/node-types.json");

/// Syntax highlighting query
pub const HIGHLIGHTS_QUERY: &str = include_str!("../../queries/highlights.scm");

/// Local scopes and definitions, for resolving references
pub const LOCALS_QUERY: &str = include_str!("../../queries/locals.scm");

/// Symbol tags, for `tree-sitter tags` and code navigation
pub const TAGS_QUERY: &str = include_str!("../../queries/tags.scm");

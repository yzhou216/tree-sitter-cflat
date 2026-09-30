// SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pre-order walk over a parse tree.
//!
//! The examples share this through `#[path]` because it is a detail of these
//! tools rather than part of the grammar's API.  It is iterative because a C♭
//! tree can be far deeper than the stack: `((((...1...))))` nests one level
//! per parenthesis.

use tree_sitter::{Node, Tree};

/// Every node of `tree` in source order, parents before children.
///
/// A node for which `descend` returns false is yielded, but its subtree is
/// skipped.
pub fn nodes<'t>(
    tree: &'t Tree,
    mut descend: impl FnMut(Node<'t>) -> bool,
) -> impl Iterator<Item = Node<'t>> {
    gen move {
        let mut cursor = tree.walk();
        loop {
            let node = cursor.node();
            yield node;
            if descend(node) && cursor.goto_first_child() {
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

// SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    env,
    path::{Path, PathBuf},
};

fn main() {
    let src = Path::new("src");
    println!("cargo::rerun-if-changed={}", src.display());

    let mut build = cc::Build::new();
    build.std("c11").include(src).file(src.join("parser.c"));

    // Build scripts see the host's `cfg`, not the target's
    if env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|target_env| target_env == "msvc") {
        build.flag("-utf-8");
    }

    if env::var("TARGET").is_ok_and(|target| target == "wasm32-unknown-unknown") {
        let headers = env::var("DEP_TREE_SITTER_LANGUAGE_WASM_HEADERS")
            .expect("tree-sitter-language should set DEP_TREE_SITTER_LANGUAGE_WASM_HEADERS");
        let sources = env::var("DEP_TREE_SITTER_LANGUAGE_WASM_SRC")
            .map(PathBuf::from)
            .expect("tree-sitter-language should set DEP_TREE_SITTER_LANGUAGE_WASM_SRC");
        build
            .include(headers)
            .files(["stdio.c", "stdlib.c", "string.c"].map(|file| sources.join(file)));
    }

    build.compile("tree-sitter-cflat");
}

# SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
# SPDX-License-Identifier: GPL-3.0-or-later

{
  description = "Tree-sitter grammar for the C♭ programming language";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    flake-parts.url = "github:hercules-ci/flake-parts";
    flake-parts.inputs.nixpkgs-lib.follows = "nixpkgs";

    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      # Not `lib.systems.flakeExposed`, which reaches as far as armv6l-linux
      # and x86_64-freebsd and would have `nix flake check --all-systems`
      # evaluate rustc, Emacs and the tree-sitter CLI for every one of them.
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      perSystem =
        { system, ... }:
        let
          pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ inputs.rust-overlay.overlays.default ];
          };
          inherit (inputs) self;

          # Nightly for `cargo -Zscript` and the `#![feature]`s in the examples
          # and tests.
          rust = pkgs.rust-bin.selectLatestNightlyWith (
            toolchain: toolchain.default.override { extensions = [ "rust-src" ]; }
          );

          # Read from tree-sitter.json, which `buildGrammar` checks it against
          inherit ((builtins.fromJSON (builtins.readFile ./tree-sitter.json)).metadata) version;

          tree-sitter-cflat = pkgs.tree-sitter.buildGrammar {
            inherit version;
            language = "cflat";
            src = self;
          };

          cargoDeps = pkgs.rustPlatform.importCargoLock { lockFile = ./Cargo.lock; };

          rustCheck =
            { name, script }:
            pkgs.stdenv.mkDerivation {
              inherit name cargoDeps;
              src = self;
              nativeBuildInputs = [
                rust
                pkgs.rustPlatform.cargoSetupHook
              ];
              # Debug builds compile the parser at -O0, where fortify only warns
              hardeningDisable = [ "fortify" ];
              dontConfigure = true;
              dontFixup = true;
              buildPhase = ''
                runHook preBuild
                export HOME=$TMPDIR
                ${script}
                runHook postBuild
              '';
              installPhase = "touch $out";
            };

          cliCheck =
            {
              name,
              packages ? [ ],
              script,
            }:
            pkgs.runCommand name
              {
                nativeBuildInputs =
                  with pkgs;
                  [
                    tree-sitter
                    nodejs
                    stdenv.cc
                  ]
                  ++ packages;
              }
              ''
                cp -r ${self}/. source
                chmod -R u+w source
                cd source
                export HOME=$TMPDIR
                ${script}
                touch $out
              '';
        in
        {
          packages = {
            inherit tree-sitter-cflat;
            default = tree-sitter-cflat;
          };

          checks = {
            inherit tree-sitter-cflat;

            corpus = cliCheck {
              name = "cflat-corpus";
              script = ''
                cp -r src committed-src
                tree-sitter generate
                if ! diff --recursive --unified committed-src src; then
                  echo >&2
                  echo "src/ is out of date: run 'tree-sitter generate' and commit the result" >&2
                  exit 1
                fi
                rm -rf committed-src
                tree-sitter test
              '';
            };

            # `cargo package` catches an `include` list that has drifted from
            # the declared example and test targets.
            rust = rustCheck {
              name = "cflat-rust";
              script = ''
                cargo fmt --check
                rustfmt --edition 2024 --check scripts/*.rs
                cargo clippy --all-targets -- -D warnings
                cargo test --all-targets
                cargo test --doc
                cargo package --allow-dirty
              '';
            };

            tools = rustCheck {
              name = "cflat-tools";
              script = ''
                cargo build --release --example lint --example tokens --example parse-check

                cargo -Zscript scripts/corpus-sources.rs "$TMPDIR/corpus"
                for file in "$TMPDIR"/corpus/*.cb; do
                  case "$(basename "$file")" in
                    # The linter exists to reject these, except for unknown
                    # type names, which would need a symbol table.
                    over-generation--unknown-type-names*) ;;
                    over-generation--*)
                      if ./target/release/examples/lint "$file" >/dev/null; then
                        echo "the linter missed $file" >&2
                        exit 1
                      fi
                      ;;
                    *)
                      if ! ./target/release/examples/lint "$file"; then
                        echo "the linter wrongly flagged $file" >&2
                        exit 1
                      fi
                      ;;
                  esac
                done

                cargo -Zscript scripts/fuzz.rs -n 2000 --seed 1
              '';
            };

            emacs-mode = cliCheck {
              name = "cflat-ts-mode";
              packages = [
                rust
              ]
              ++ (with pkgs; [
                emacs
                gnumake
              ]);
              script = ''
                make -C emacs test
                make -C emacs stress
              '';
            };

            formatting = pkgs.runCommand "cflat-nixfmt" { nativeBuildInputs = [ pkgs.nixfmt ]; } ''
              find ${self} -name '*.nix' -print0 | xargs -0 nixfmt --check
              touch $out
            '';

            licensing = pkgs.runCommand "cflat-reuse" { nativeBuildInputs = [ pkgs.reuse ]; } ''
              reuse --root ${self} lint
              touch $out
            '';
          };

          devShells.default = pkgs.mkShell {
            packages = [
              rust
            ]
            ++ (with pkgs; [
              tree-sitter
              # `tree-sitter generate` evaluates grammar.js
              nodejs
              rust-analyzer
              stdenv.cc
              gnumake
              reuse
              emacs
            ]);

            hardeningDisable = [ "fortify" ];

            env.RUST_SRC_PATH = "${rust}/lib/rustlib/src/rust/library";

            shellHook = ''
              echo "C♭ tree-sitter grammar"
              echo "  tree-sitter $(tree-sitter --version | cut -d' ' -f2)   cargo $(cargo --version | cut -d' ' -f2)   $(emacs --version | head -1)"
              echo
              echo "  tree-sitter generate       regenerate src/parser.c from grammar.js"
              echo "  tree-sitter test           run the test/corpus suite"
              echo "  tree-sitter parse FILE     dump a parse tree"
              echo "  cargo test                 Rust bindings, queries, incremental reparse"
              echo "  make -C emacs test         cflat-ts-mode's ERT suite"
              echo "  scripts/fuzz.rs            generate random programs and check them"
              echo "  scripts/cross-validate.rs  diff token streams against the reference lexer"
              echo "  scripts/check-all.rs       every check, using whatever is on PATH"
              echo "  reuse lint                 check SPDX headers"
              echo "  nix flake check            everything CI runs"
            '';
          };

          formatter = pkgs.nixfmt;
        };
    };
}

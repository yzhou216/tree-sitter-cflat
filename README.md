# tree-sitter-cflat

[Tree-sitter][] grammar for C♭, the toy language from the USF CS 414
compilers course (RISC-V and amd64 backends), plus a GNU Emacs major mode
for editing C♭ source files.

[tree-sitter]: https://tree-sitter.github.io/tree-sitter/

## Quick start

```sh
nix develop                # tree-sitter, node, rust, emacs, a C compiler
tree-sitter generate       # regenerate src/parser.c from grammar.js
tree-sitter test           # the corpus suite
cargo test                 # bindings, queries, coverage, incremental reparse
make -C emacs test         # cflat-ts-mode's ERT suite

scripts/check-all.rs       # all of the above, plus fuzzing and, given the
                           # course compiler, lexer cross-validation
```

Without Nix you need the `tree-sitter` CLI, a JS runtime (the CLI evaluates
`grammar.js`), a C compiler, a nightly Rust toolchain (the scripts, examples
and tests use unstable features; the library itself builds on stable), and for
the editor mode Emacs 31+ built with tree-sitter support.

## Layout

| Path                      | What it is                                     |
| ------------------------- | ---------------------------------------------- |
| `cflat.cfg`               | reference CFG this grammar implements          |
| `grammar.js`              | tree-sitter grammar, the only hand-edited file |
| `src/`                    | generated parser                               |
| `queries/`                | highlights, locals and tags queries            |
| `test/corpus/`            | `tree-sitter test` parse-tree cases            |
| `test/highlight/`         | inline highlight assertions                    |
| `test/rust/`              | Rust integration tests                         |
| `examples/tokens.rs`      | token-stream dumper, for cross-validation      |
| `examples/lint.rs`        | rules the grammar cannot enforce               |
| `examples/parse-check.rs` | batch parse checker used by the fuzzer         |
| `examples/walk.rs`        | tree walker shared by the three examples       |
| `bindings/rust/`          | `tree_sitter_cflat::LANGUAGE`                  |
| `emacs/`                  | `cflat-ts-mode` and its tests                  |
| `scripts/`                | corpus extraction, cross-validation, fuzzer    |
| `flake.nix`               | dev shell, package, and `nix flake check`      |

Rust is the only binding; the rest are disabled in `tree-sitter.json`, so there
is no `CMakeLists.txt`, `Makefile` or `bindings/c/`. Anything that just needs a
shared library can compile `src/parser.c` directly, which is what
`nix build .#tree-sitter-cflat` and `emacs/Makefile` do. `package.json` is
unpublished and exists only to pin the CLI and let it evaluate `grammar.js` as
an ES module.

The Rust integration tests live in `test/rust/`, not Cargo's `tests/`. The
tree-sitter CLI hardcodes `test/corpus` and `test/highlight`, and one test
directory beats two, so `Cargo.toml` names the five targets by hand.

## Language notes

`cflat.cfg` is the reference grammar from the course notes; `grammar.js`
implements it. The places where C♭ surprises people, all pinned by
`test/corpus/`.

**Precedence**, loosest to tightest: ternary `? :`, `and`/`or`,
`== != < <= > >=`, `+ -`, `* /`, unary, postfix (calls, `.field`, `.*`,
`[index]`).

**Associativity**: unary and `and`/`or` are right-associative; everything else,
including the ternary, is left-associative. So `a ? b : c ? d : e` parses as
`(a ? b : c) ? d : e`, not the C reading. `and` and `or` share one precedence
level.

**Allocation** has two unrelated forms: `new type` for a singleton,
`[type ; exp]` for an array. There is no `new [type; exp]`, and `new` is an
ordinary primary expression rather than something restricted to the right of an
assignment.

**Assignment is a statement**: `stmt ::= exp ; | exp = exp ;`. It does not nest
(`foo(x = 1)`) and does not chain (`x = y = 1`).

**Lists** are `LIST(α) = (α (',' α)*)?`. The empty list is fine, so `let;`,
`fn f()`, `struct S {}` and `f()` all parse, but a trailing comma is not.

**`let` declarations** are function-local and must all precede the statements
in a function body. Blocks (`if`/`while` bodies) may not contain them.

**Whitespace** is exactly `` ``, `\t`, `\n` and `\r`, not the wider set `\s`
matches, so a form feed or vertical tab is a syntax error. Note the asymmetry
with comments: `\r` is whitespace, but `comment ::= '//' [^\n]* '\n'` ends a
line comment at a newline specifically. In a file with lone-CR line endings a
`//` comment therefore runs to end of input and swallows the rest of the
program. The reference lexer emits a single error token over the same span.

## Unterminated comments

Both comment forms must be terminated, which makes a `//` on the last line of a
file with no trailing newline *not* a comment.

The reference grammar calls an unterminated comment a lexing error. Since that
has no effect on the shape of the tree, this grammar gives it its own node,
`unterminated_comment`, in `extras` rather than letting it become an `ERROR`.
The rest of the file stays parseable, and both `queries/highlights.scm` and
`cflat-ts-mode` paint the region as an error. To treat it as a hard failure,
match the node:

```scm
(unterminated_comment) @error
```

Tree-sitter's regex engine has no lazy quantifier, so "up to the *first* `*/`"
is spelled out as a pattern that provably cannot contain `*/`. The two comment
tokens cannot shadow each other: a terminated match is always strictly longer
than the unterminated match at the same position, and the lexer takes the
longest match.

## What this grammar deliberately does not enforce

The reference CFG in `cflat.cfg` over-generates, and the real C♭ parser
rejects the surplus while building the AST rather than while parsing. A
syntax-only grammar cannot express any of the following, so it accepts them:

- **Expression statements must be function calls.** `x;` and `1 + 2;` parse.
- **Assignment left-hand sides must be places** (a variable, `.field`, `.*` or
  `[index]`). `x + y = 17;` parses.
- **No duplicate top-level, field, parameter or local names.**
- **Type names must resolve.** `bool` and `string` are just struct types here.

Contorting the grammar to express these would mean abandoning the reference CFG
and would still not be sound, since the last two need a symbol table. They
belong in a lint layer over the tree. `test/corpus/over-generation.txt`
documents the exact set.

To make that layer easy to write, `_expression`, `_statement` and `_type` are
declared as supertypes. They are hidden rules that never appear in a tree, but
a query can match them generically, so "find every assignment target" is one
pattern rather than a thirteen-way alternation:

```scm
(assignment left: (_expression) @target)
```

`examples/lint.rs` is that layer written out. It enforces the first three rules,
including the notes' subtlety that an identically repeated declaration collapses
to one rather than erroring, and is tested against the programs
`over-generation.txt` documents. The fourth needs a symbol table and a type
checker, which is the compiler's job.

```console
$ cargo run --quiet --example lint -- prog.cb
prog.cb:3:3: expression statement is not a function call
prog.cb:4:3: assignment target is not a place (a variable, `.field`, `.*` or `[index]`)
```

The course notes define a place as `exp.*`, `exp[exp]` or `exp.id`, leaving a
bare identifier off the list. It plainly belongs, since the same page's examples
include `x = 0;` and `while x > 0 { x = x - 1; }`, and without it no local could
ever be assigned to. The linter treats the omission as an oversight.

## Editor support

`emacs/` holds `cflat-ts-mode`: font-lock in four levels, indentation, `imenu`,
`which-function-mode`, `outline-minor-mode`, `hs-minor-mode`, comment filling
and structural navigation. See [emacs/README.md](emacs/README.md).

`queries/` is editor-agnostic:

| File             | Purpose                                   |
| ---------------- | ----------------------------------------- |
| `highlights.scm` | syntax highlighting                       |
| `locals.scm`     | scopes and bindings, for go-to-definition |
| `tags.scm`       | `tree-sitter tags`, for symbol search     |

## Testing

Six layers, all runnable from `nix develop`.

**1. Corpus.** `tree-sitter test`, 101 cases across `test/corpus/`: types,
declarations, statements, every operator, every precedence and associativity
relation, the comment forms, the three programs from the course repo's
`test-inputs/`, an `over-generation.txt` of what the grammar knowingly accepts,
a `rejections.txt` of forms that must error, and an `edge-cases.txt` of
identifiers beginning with keywords, long operator and postfix chains, and
unusual whitespace.

**2. Highlight assertions.** Also `tree-sitter test`, from `test/highlight/`: 37
inline assertions that a given column carries a given capture. Query precedence
is easy to get backwards, since tree-sitter keeps the capture appearing *last*
when several land on one node. A catch-all `(identifier) @variable` at the end
of the file silently repaints every type, function and property name; that bug
and a `@spell` hint shadowing `@comment` were both caught here.

**3. Rust integration.** `cargo test`, covering what the corpus format cannot:

- *Fields and operator text.* Corpus expectations print only named nodes, so
  `a + b` and `a - b` have identical expected trees and the `field(...)`
  annotations are never checked.
- *Input not ending in a newline.* A corpus entry's source is always
  newline-terminated before the `---` divider, making the unterminated trailing
  line comment unreachable there.
- *Query compilation.* `queries/*.scm` reference node names as strings, and
  nothing connects them to `grammar.js` until something compiles them.
  `test/rust/queries.rs` does, and checks the captures actually fire.
- *Coverage.* `test/rust/coverage.rs` asks the compiled grammar what it can
  produce and asserts the corpus reaches all of it: currently 39/39 node kinds
  and 23/23 fields.
- *Pathological input.* `test/rust/stress.rs` builds 2,000-deep nestings and
  20,000-term operator chains and checks the tree is not merely clean but
  complete, with the right node count nested the right way, plus that error
  recovery terminates on 200 KB of garbage. A quadratic regression in the
  precedence table would surface as the suite hanging.
- *Incremental reparse.* `test/rust/incremental.rs` performs each edit both
  incrementally and from scratch, then compares the trees. It covers every
  single-character deletion of a sample program, every insertion of each lexical
  class, typing a program in one character at a time, deleting one back out, and
  the comment-boundary edits where `extras` reuse is most likely to go wrong.

**4. Emacs.** `make -C emacs test`, 50 ERT tests: indentation idempotence and
recovery, every font-lock capture at every `treesit-font-lock-level`, `imenu`,
defun and list navigation, folding, comment continuation, and robustness
against half-typed buffers.

`make -C emacs stress` adds a cross-layer property: reindenting only moves
whitespace, so the parse tree must come out identical and indentation must be a
fixed point. It runs over the extracted corpus by default, or over a larger
sample:

```sh
scripts/fuzz.rs -n 2000 --dump /tmp/generated
make -C emacs stress STRESS_DIR=/tmp/generated
```

**5. Cross-validation against the reference lexer.** The course compiler's
hand-written lexer is the authoritative implementation of C♭'s lexical layer, so
agreeing with it token-for-token is the strongest available check on `extras`,
keyword extraction and the comment patterns:

```sh
scripts/cross-validate.rs /path/to/cflat-compiler
```

All 87 valid corpus programs match it exactly.

Only valid C♭ is cross-checked. Tree-sitter's lexer is context-sensitive, since
it considers only the tokens legal in the current parse state, so on input the
grammar rejects it can legitimately disagree with a standalone lexer: a `let`
where no `let` may appear comes back as an `identifier` rather than the `Let`
keyword. That divergence is inherent to the approach and only shows up on input
that is not C♭ to begin with.

**6. Fuzzing.** `scripts/fuzz.rs` generates random programs straight from the
reference CFG and checks each parses with no error and, given a reference
compiler, lexes identically. It then corrupts each one and checks the parser
still terminates quietly. A fraction of the programs get a deliberately
unterminated trailing comment or CRLF line endings, both of which must still
parse and still lex identically.

```sh
cargo build --release --example tokens --example parse-check
scripts/fuzz.rs -n 2000 --reference /path/to/cflat-compiler
```

It drives the parser through those two statically-linked helpers rather than the
CLI. The CLI reloads the grammar from the working directory and rebuilds it when
`src/parser.c` looks newer, so a campaign running alongside a `tree-sitter
generate` reports failures that are really the parser being swapped underneath
it. Linking it in statically removes the race and runs about three times faster.

### Running everything

`scripts/check-all.rs` is the fast local sweep, using whatever is on `$PATH`.
Given the reference compiler it adds the cross-validation and the fuzzer's
token-stream comparison.

CI runs one command instead:

```sh
nix flake check
```

Every check is a flake output built against the `flake.lock` pins, so a green
run locally is the run CI gets: same rustc, same tree-sitter CLI, same Emacs.

| Check               | What it runs                                            |
| ------------------- | ------------------------------------------------------- |
| `corpus`            | `tree-sitter test`, plus a guard that `src/` is current |
| `rust`              | fmt, clippy `-D warnings`, tests, doc tests, `package`  |
| `tools`             | linter over every corpus program, 2000 fuzz cases       |
| `emacs-mode`        | byte-compile, ERT suite, indent stress run              |
| `tree-sitter-cflat` | grammar built as a shared library                       |
| `formatting`        | `nixfmt --check` over the flake                         |
| `licensing`         | `reuse lint`                                            |

## License

`GPL-3.0-or-later`, except the tree-sitter runtime headers under
`src/tree_sitter/`, which the CLI copies in verbatim and which stay `MIT`. The
repository is [REUSE](https://reuse.software/) compliant; `REUSE.toml` covers
the files with nowhere to put a header.

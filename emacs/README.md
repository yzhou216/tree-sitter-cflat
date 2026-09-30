# cflat-ts-mode

Emacs major mode for C♭, built on Emacs 31's tree-sitter support and the
grammar in the parent directory.

Provides font-lock, indentation, `imenu`, `which-function-mode`,
`outline-minor-mode`, `hs-minor-mode`, comment filling and structural
navigation. Files ending in `.cb` use it automatically, and opening one offers
to build the grammar if it is missing.

## Requirements

- Emacs 31.1 or newer, built with tree-sitter support. Check with
  `M-: (treesit-available-p)`.
- Compiled `cflat` grammar, which Emacs can build itself.

## Installation

```elisp
(add-to-list 'load-path "/path/to/tree-sitter-cflat/emacs")
(require 'cflat-ts-mode)
```

### Building the grammar from Emacs

Loading the mode registers the grammar's recipe in
`treesit-language-source-alist`, so visiting a `.cb` file without the grammar
builds it as `treesit-auto-install-grammar` directs, by default after asking.
To build or rebuild it explicitly:

```
M-x treesit-install-language-grammar RET cflat RET
```

The recipe points at the upstream repository. While hacking on the grammar,
put a local checkout first instead:

```elisp
(add-to-list 'treesit-language-source-alist
             '(cflat "/home/you/Projects/tree-sitter-cflat"))
```

### Using a grammar built elsewhere

If the shared library is already built, say by `nix build .#tree-sitter-cflat`
or `make -C emacs grammar` into `emacs/.grammar/`, put its directory on the
load path:

```elisp
(add-to-list 'treesit-extra-load-path "/path/to/directory/containing/the/so")
```

By hand, from the repository root:

```sh
cc -shared -fPIC -O2 -std=c11 -I src src/parser.c -o libtree-sitter-cflat.so
```

## Customization

| Option                         | Default | Meaning                            |
| ------------------------------ | ------- | ---------------------------------- |
| `cflat-ts-indent-offset`       | `2`     | spaces per indentation step        |
| `treesit-auto-install-grammar` | `ask`   | whether to build a missing grammar |

Font-lock detail follows `treesit-font-lock-level` (default 3):

| Level | Adds                                               |
| ----- | -------------------------------------------------- |
| 1     | comments, definition names                         |
| 2     | keywords, types                                    |
| 3     | constants, numbers, calls, properties              |
| 4     | operators, brackets, delimiters, variables, errors |

## Notes

**Unterminated comments are painted as errors.** C♭ treats them as lexing
errors, and the grammar reports one as a distinct `unterminated_comment` node
rather than failing the parse. This mode gives that node
`font-lock-warning-face`, so a stray `/*`, or a `//` on the last line of a file
with no trailing newline, is visible immediately instead of silently swallowing
the rest of the buffer.

**The ternary is left-associative.** `a ? b : c ? d : e` groups as
`(a ? b : c) ? d : e`, unlike C. Nothing in the mode depends on this, but it is
the thing most likely to surprise you when reading a tree with
`M-x treesit-explore`.

## Development

```sh
make compile   # byte-compile, warnings are errors
make test      # build the grammar, then run the ERT suite
make clean
```

`make test` builds its own copy of the grammar into `emacs/.grammar/`. To reuse
one you already have:

```sh
make test GRAMMAR_DIR=/path/to/directory
```

The suite covers mode activation, the grammar recipe, comment syntax and
continuation, indentation (idempotence, recovery from flattened and
over-indented source, `else` alignment, nesting, a custom offset), font-lock
for every capture the mode defines and for each `treesit-font-lock-level`,
`imenu`, defun and list navigation, folding, and robustness against half-typed
buffers.

`make stress` additionally reindents a whole corpus and checks the parse tree
never changes and that indentation is a fixed point.

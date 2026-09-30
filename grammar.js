// SPDX-FileCopyrightText: 2026 Yiyu Zhou <yzhou155@dons.usfca.edu>
// SPDX-License-Identifier: GPL-3.0-or-later

/**
 * @file C♭ grammar for tree-sitter
 * @author Yiyu Zhou <yzhou155@dons.usfca.edu>
 * @license GPL-3.0-or-later
 *
 * Follows the reference CFG in cflat.cfg, including where it deliberately
 * over-generates.  The rules it leaves to the compiler belong in a lint
 * layer; see README.md.
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const PREC = {
  ternary: 1,
  logical: 2,
  relational: 3,
  additive: 4,
  multiplicative: 5,
  unary: 6,
  postfix: 7,
};

// Narrower than \s: the reference admits no form feed or vertical tab
const WHITESPACE = /[ \t\r\n]/;

// The reference ends a block comment at the first `*/`.  Tree-sitter's regex
// engine has no lazy quantifier, so this spells out a body that provably
// contains no `*/`:
//
//   [^*]*            a run with no stars at all, then
//   (\*+[^*/][^*]*)* zero or more star-runs, each followed by a character that
//                    is neither `*` nor `/` and then more star-free text, and
//   \**              a trailing star-run at the very end.
//
// Every star-run is followed by something other than `/` or ends the match,
// so `*/` cannot occur inside it.
const BLOCK_COMMENT_BODY = /[^*]*(\*+[^*/][^*]*)*\**/;

export default grammar({
  name: 'cflat',

  word: $ => $.identifier,

  supertypes: $ => [$._expression, $._statement, $._type],

  extras: $ => [
    WHITESPACE,
    $.comment,
    // An unterminated comment is a lexing error, but one that cannot change
    // the shape of the tree.  As an extra it leaves the rest of the file
    // parseable and gives queries a precise node to flag.
    $.unterminated_comment,
  ],

  rules: {
    // The reference grammar has no empty program, so an empty file is an
    // error.
    source_file: $ => repeat1($._top_level_item),

    _top_level_item: $ => choice(
      $.struct_declaration,
      $.extern_declaration,
      $.function_definition,
    ),

    _type: $ => choice(
      $.primitive_type,
      $.struct_type,
      $.pointer_type,
      $.array_type,
      $.function_type,
    ),

    primitive_type: _ => 'int',

    struct_type: $ => field('name', $.identifier),

    pointer_type: $ => seq('&', field('pointee', $._type)),

    array_type: $ => seq('[', field('element', $._type), ']'),

    function_type: $ => seq(
      field('parameters', $.type_parameter_list),
      '->',
      field('return_type', $._type),
    ),

    type_parameter_list: $ => seq('(', list($._type), ')'),

    declaration: $ => seq(
      field('name', $.identifier),
      ':',
      field('type', $._type),
    ),

    struct_declaration: $ => seq(
      'struct',
      field('name', $.identifier),
      field('body', $.field_declaration_list),
    ),

    field_declaration_list: $ => seq('{', list($.declaration), '}'),

    extern_declaration: $ => seq(
      'extern',
      field('name', $.identifier),
      ':',
      field('type', $.function_type),
      ';',
    ),

    function_definition: $ => seq(
      'fn',
      field('name', $.identifier),
      field('parameters', $.parameter_list),
      '->',
      field('return_type', $._type),
      field('body', $.function_body),
    ),

    parameter_list: $ => seq('(', list($.declaration), ')'),

    function_body: $ => seq(
      '{',
      repeat($.let_declaration),
      repeat($._statement),
      '}',
    ),

    let_declaration: $ => seq('let', list($.declaration), ';'),

    _statement: $ => choice(
      $.expression_statement,
      $.assignment,
      $.if_statement,
      $.while_statement,
      $.break_statement,
      $.continue_statement,
      $.return_statement,
    ),

    // The reference parser also demands a call here, a check left to
    // examples/lint.rs.
    expression_statement: $ => seq(field('expression', $._expression), ';'),

    // The reference parser also demands that `left` be a place, a check left
    // to examples/lint.rs.
    assignment: $ => seq(
      field('left', $._expression),
      '=',
      field('right', $._expression),
      ';',
    ),

    if_statement: $ => seq(
      'if',
      field('condition', $._expression),
      field('consequence', $.block),
      optional(seq('else', field('alternative', $.block))),
    ),

    while_statement: $ => seq(
      'while',
      field('condition', $._expression),
      field('body', $.block),
    ),

    break_statement: _ => seq('break', ';'),

    continue_statement: _ => seq('continue', ';'),

    return_statement: $ => seq('return', field('value', $._expression), ';'),

    block: $ => seq('{', repeat($._statement), '}'),

    _expression: $ => choice(
      $.identifier,
      $.number,
      $.nil,
      $.unary_expression,
      $.binary_expression,
      $.ternary_expression,
      $.allocation,
      $.array_allocation,
      $.call_expression,
      $.field_access,
      $.pointer_access,
      $.index_expression,
      $.parenthesized_expression,
    ),

    // cflat.cfg has `ternary ::= logical ('?' exp ':' logical)*`, and the
    // Kleene star makes the ternary LEFT-associative, unlike C's:
    // `a ? b : c ? d : e` is `(a ? b : c) ? d : e`.  `prec.left` reproduces
    // that by reducing the finished ternary when another `?` follows.  The
    // consequence is a full `exp`, so a ternary nests there without any
    // associativity choice to make.
    ternary_expression: $ => prec.left(PREC.ternary, seq(
      field('condition', $._expression),
      '?',
      field('consequence', $._expression),
      ':',
      field('alternative', $._expression),
    )),

    binary_expression: $ => {
      /** @type {[(p: number, r: RuleOrLiteral) => Rule, number, string[]][]} */
      const table = [
        [prec.right, PREC.logical, ['and', 'or']],
        [prec.left, PREC.relational, ['==', '!=', '<', '<=', '>', '>=']],
        [prec.left, PREC.additive, ['+', '-']],
        [prec.left, PREC.multiplicative, ['*', '/']],
      ];

      return choice(...table.flatMap(([assoc, precedence, operators]) =>
        operators.map(operator => assoc(precedence, seq(
          field('left', $._expression),
          field('operator', operator),
          field('right', $._expression),
        )))));
    },

    unary_expression: $ => prec.right(PREC.unary, seq(
      field('operator', choice('-', 'not')),
      field('operand', $._expression),
    )),

    call_expression: $ => prec(PREC.postfix, seq(
      field('function', $._expression),
      field('arguments', $.argument_list),
    )),

    argument_list: $ => seq('(', list($._expression), ')'),

    index_expression: $ => prec(PREC.postfix, seq(
      field('array', $._expression),
      '[',
      field('index', $._expression),
      ']',
    )),

    // cflat.cfg left-factors these two behind `postfix_dot` so its LL(1)
    // parser can decide after the `.`, which an LR parser does not need.
    field_access: $ => prec.left(PREC.postfix, seq(
      field('object', $._expression),
      '.',
      field('field', $.identifier),
    )),

    pointer_access: $ => prec.left(PREC.postfix, seq(
      field('object', $._expression),
      '.',
      '*',
    )),

    allocation: $ => seq('new', field('type', $._type)),

    array_allocation: $ => seq(
      '[',
      field('type', $._type),
      ';',
      field('size', $._expression),
      ']',
    ),

    parenthesized_expression: $ => seq('(', $._expression, ')'),

    nil: _ => 'nil',

    identifier: _ => /[a-zA-Z][a-zA-Z0-9_]*/,

    number: _ => /[0-9]+/,

    // Both forms must be terminated to be a comment, and a line comment's
    // terminator is its newline, so a `//` on a final line with no newline is
    // an `unterminated_comment` instead.
    comment: _ => token(choice(
      seq('//', /[^\n]*/, '\n'),
      seq('/*', BLOCK_COMMENT_BODY, '*/'),
    )),

    // Never shadows `comment`.  At any position the terminated match is the
    // unterminated one plus its `\n` or `*/`, so the longest match prefers
    // it.  BLOCK_COMMENT_BODY cannot cross a `*/`, so the block form here wins
    // only when no `*/` follows at all.
    unterminated_comment: _ => token(choice(
      seq('//', /[^\n]*/),
      seq('/*', BLOCK_COMMENT_BODY),
    )),
  },
});

/**
 * Comma-separated list, possibly empty, with no trailing comma.
 *
 * @param {RuleOrLiteral} rule
 * @returns {ChoiceRule}
 */
function list(rule) {
  return optional(seq(rule, repeat(seq(',', rule))));
}

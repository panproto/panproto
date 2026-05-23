/// Tree-sitter grammar for the BUGS probabilistic programming language.
///
/// Covers WinBUGS / OpenBUGS syntax. Reference: the BUGS Reference
/// Manual (Lunn et al., 2012).
///
/// BUGS programs consist of a single `model { ... }` block containing
/// stochastic (`~`) and deterministic (`<-`) relations, for-loops,
/// and nested blocks.

module.exports = grammar({
  name: "bugs",

  extras: ($) => [/\s/, $.comment],

  conflicts: ($) => [
    [$.function_call, $.indexed_variable],
  ],

  rules: {
    source_file: ($) => repeat($.model_block),

    model_block: ($) =>
      seq(
        optional("model"),
        "{",
        repeat($._statement),
        "}"
      ),

    _statement: ($) =>
      choice(
        $.stochastic_relation,
        $.deterministic_relation,
        $.for_loop,
        $.block
      ),

    block: ($) => seq("{", repeat($._statement), "}"),

    // x ~ dnorm(0, 1)
    stochastic_relation: ($) =>
      seq(
        field("variable", $._expression),
        "~",
        field("distribution", $.distribution_call),
        optional($.truncation)
      ),

    // x <- a + b
    deterministic_relation: ($) =>
      seq(
        field("variable", $._expression),
        "<-",
        field("value", $._expression)
      ),

    for_loop: ($) =>
      seq(
        "for",
        "(",
        field("variable", $.identifier),
        "in",
        field("range", $.range),
        ")",
        field("body", $.block)
      ),

    range: ($) =>
      seq(
        field("lower", $._expression),
        ":",
        field("upper", $._expression)
      ),

    distribution_call: ($) =>
      seq(
        field("name", $.identifier),
        "(",
        field("arguments", optional($.argument_list)),
        ")"
      ),

    truncation: ($) =>
      choice(
        seq("T", "(", $._expression, ",", $._expression, ")"),
        seq("I", "(", $._expression, ",", $._expression, ")"),
        seq("C", "(", $._expression, ",", $._expression, ")")
      ),

    argument_list: ($) =>
      seq($._expression, repeat(seq(",", $._expression))),

    _expression: ($) =>
      choice(
        $.binary_expression,
        $.unary_expression,
        $.function_call,
        $.indexed_variable,
        $.identifier,
        $.number,
        $.parenthesized_expression
      ),

    parenthesized_expression: ($) =>
      seq("(", $._expression, ")"),

    binary_expression: ($) =>
      choice(
        prec.left(1, seq(field("left", $._expression), field("operator", "+"), field("right", $._expression))),
        prec.left(1, seq(field("left", $._expression), field("operator", "-"), field("right", $._expression))),
        prec.left(2, seq(field("left", $._expression), field("operator", "*"), field("right", $._expression))),
        prec.left(2, seq(field("left", $._expression), field("operator", "/"), field("right", $._expression))),
        prec.right(3, seq(field("left", $._expression), field("operator", "^"), field("right", $._expression))),
        prec.left(0, seq(field("left", $._expression), field("operator", "=="), field("right", $._expression))),
        prec.left(0, seq(field("left", $._expression), field("operator", "!="), field("right", $._expression))),
        prec.left(0, seq(field("left", $._expression), field("operator", "<"), field("right", $._expression))),
        prec.left(0, seq(field("left", $._expression), field("operator", ">"), field("right", $._expression))),
        prec.left(0, seq(field("left", $._expression), field("operator", "<="), field("right", $._expression))),
        prec.left(0, seq(field("left", $._expression), field("operator", ">="), field("right", $._expression))),
      ),

    unary_expression: ($) =>
      prec(4, seq(choice("-", "+"), field("operand", $._expression))),

    function_call: ($) =>
      prec(5, seq(
        field("name", $.identifier),
        "(",
        field("arguments", optional($.argument_list)),
        ")"
      )),

    indexed_variable: ($) =>
      prec(5, seq(
        field("name", $.identifier),
        "[",
        field("indices", $.index_list),
        "]"
      )),

    index_list: ($) =>
      seq($._index_element, repeat(seq(",", $._index_element))),

    _index_element: ($) => choice($._expression, $.range),

    identifier: ($) => /[a-zA-Z_][a-zA-Z0-9_.]*[a-zA-Z0-9_]|[a-zA-Z_]/,

    number: ($) =>
      token(
        choice(
          /[0-9]+(\.[0-9]*)?([eE][+-]?[0-9]+)?/,
          /\.[0-9]+([eE][+-]?[0-9]+)?/
        )
      ),

    comment: ($) => token(seq("#", /.*/)),
  },
});

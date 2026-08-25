/**
 * Tree-sitter grammar for Strudel mini-notation.
 *
 * Authored from the official Strudel documentation at:
 *
 *   https://strudel.cc/learn/mini-notation/
 *
 * Strudel's mini-notation is a port of TidalCycles' (also implemented
 * here as `tree-sitter-tidal-mini`). The two languages share most of
 * their surface; the documented differences from Tidal — verified
 * against the Strudel docs — are:
 *
 *   - `-` is accepted as an alternative spelling of the rest token,
 *     in addition to `~`.
 *   - The Tidal-specific `_` elongation marker is *not* in the
 *     Strudel docs; only the `@` elongation suffix is.
 *   - The Tidal-specific `{}` polymetric brackets are *not* in the
 *     Strudel docs.
 *   - The Tidal-specific `%N` numeric ratio suffix is *not* in the
 *     Strudel docs.
 *   - The Tidal-specific top-level `.` grouping shorthand is *not* in
 *     the Strudel docs.
 *
 * The grammar otherwise mirrors `tree-sitter-tidal-mini`; per-rule
 * comments cite the Strudel docs section that licensed the rule.
 */

module.exports = grammar({
  name: 'strudel_mini',

  extras: $ => [/[ \t\n\r]+/],

  word: $ => $.identifier,

  rules: {
    // Strudel allows the random-choice `|` and superposition `,`
    // separators at the top level too, per the docs example
    // `note("[g3,b3,e4] | [a3,c3,e4]")`. Tidal restricts those to
    // inside containers.
    source_file: $ => optional($._pattern_list),

    // No top-level dot in Strudel; a single contiguous run is a flat
    // sequence (per the docs: events separated by whitespace).
    _pattern: $ => repeat1($._step),

    _step: $ => choice(
      $._suffixed_step,
      $.rest,
    ),

    _suffixed_step: $ => seq(
      $._step_head,
      repeat($._suffix),
    ),

    _step_head: $ => choice(
      $._atom,
      $.group,
      $.alternation,
    ),

    _atom: $ => choice(
      $.event,
      $.number,
    ),

    // Sample selection per the Strudel docs: `note("c e g b")` and
    // `s("bd:1 bd:2")` style.
    event: $ => seq(
      field('name', $.identifier),
      optional(seq(':', field('sample', $.number))),
    ),

    // Per the docs: "rest/silence" is `~` *or* `-`. Both spellings are
    // accepted; consumers can normalize downstream.
    rest: $ => choice('~', '-'),

    group: $ => seq('[', $._pattern_list, ']'),

    alternation: $ => seq('<', $._pattern_list, '>'),

    _pattern_list: $ => seq(
      $._pattern,
      repeat(seq(
        choice(',', '|'),
        $._pattern,
      )),
    ),

    _suffix: $ => choice(
      $.repeat_suffix,
      $.divide_suffix,
      $.elongate_suffix,
      $.replicate_suffix,
      $.probability_suffix,
      $.euclid_suffix,
    ),

    repeat_suffix: $ => seq('*', alias(token.immediate(/[0-9]+(\.[0-9]+)?/), $.number)),

    divide_suffix: $ => seq('/', alias(token.immediate(/[0-9]+(\.[0-9]+)?/), $.number)),

    elongate_suffix: $ => seq('@', alias(token.immediate(/[0-9]+(\.[0-9]+)?/), $.number)),

    replicate_suffix: $ => seq('!', optional(alias(token.immediate(/[0-9]+(\.[0-9]+)?/), $.number))),

    probability_suffix: $ => seq('?', optional(alias(token.immediate(/[0-9]+(\.[0-9]+)?/), $.number))),

    euclid_suffix: $ => seq(
      '(',
      field('beats', $.number),
      ',',
      field('steps', $.number),
      optional(seq(',', field('offset', $.number))),
      ')',
    ),

    // Identifiers are letter-leading and may continue with letters,
    // digits, or underscores.
    identifier: $ => /[a-zA-Z][a-zA-Z0-9_]*/,

    number: $ => /[0-9]+(\.[0-9]+)?/,
  },
});

/**
 * Tree-sitter grammar for TidalCycles mini-notation.
 *
 * Authored from the official spec at:
 *
 *   https://tidalcycles.org/docs/reference/mini_notation
 *
 * Every construct in this grammar is grounded in a documented example
 * from that page; the comment on each rule cites the section it came
 * from. Operators and bracket types not appearing in the docs are not
 * included.
 *
 * Mini-notation is the *island* grammar embedded inside the string
 * argument to `s`, `n`, `note`, etc. in a Haskell-host Tidal program.
 * This grammar's `pattern` rule corresponds to the contents of one
 * such string; consumers wrap or strip the surrounding quotes.
 *
 * Top-level shape (per the docs):
 *   pattern  := step (sep step)*
 *   sep      := whitespace | "."   // dot is top-level grouping shorthand
 *   step     := atom suffix*
 *             | "[" pattern_list "]"
 *             | "<" pattern_list ">"
 *             | "{" pattern_list ("%" number)? "}"
 *             | "(" euclid ")"      // not standalone; only as a step suffix
 *             | "~"                  // rest
 *             | "_"                  // elongation marker
 *   atom     := identifier (":" number)?  // sample selection per docs
 *             | number
 *   suffix   := "*" number
 *             | "/" number
 *             | "@" number           // elongation
 *             | "!" number?          // replication
 *             | "?" probability?     // probabilistic removal
 *             | "%" number           // numeric ratio (rare)
 *             | "(" euclid ")"       // euclidean rhythm
 *   euclid   := number "," number ("," number)?  // (beats, steps[, offset])
 */

module.exports = grammar({
  name: 'tidal_mini',

  extras: $ => [/[ \t\n\r]+/],

  conflicts: $ => [],

  word: $ => $.identifier,

  rules: {
    // The whole pattern is a sequence of steps, possibly separated by
    // top-level dots (`.`) which the docs describe as grouping shorthand:
    // `"bd*3 . hh*4 cp"` ≡ `"[bd*3] [hh*4 cp]"`.
    source_file: $ => optional($._pattern),

    _pattern: $ => seq(
      $._dot_group,
      repeat(seq('.', $._dot_group)),
    ),

    _dot_group: $ => repeat1($._step),

    // A step is one positional element in the cycle. The bracket types
    // produce composite steps; the leaf forms are atoms, rests, and
    // elongation markers.
    _step: $ => choice(
      $._suffixed_step,
      $.rest,
      $.elongation,
    ),

    // A suffix-bearing step: any `_step_head` followed by zero or more
    // suffixes. We split the head out so suffixes can attach to atoms,
    // brackets, or other group forms uniformly.
    _suffixed_step: $ => seq(
      $._step_head,
      repeat($._suffix),
    ),

    _step_head: $ => choice(
      $._atom,
      $.group,
      $.alternation,
      $.polymetric,
    ),

    // Atom forms: an identifier (sample/event name) optionally followed
    // by a colon-and-number for sample selection (per the docs:
    // `"arpy:1 arpy:2 arpy:3"`).
    _atom: $ => choice(
      $.event,
      $.number,
    ),

    event: $ => seq(
      field('name', $.identifier),
      optional(seq(':', field('sample', $.number))),
    ),

    // `~` is the documented rest token. No suffixes attach to a rest in
    // the corpus examples, so we keep it as a standalone step.
    rest: $ => '~',

    // `_` extends the previous step. Per the docs:
    // `"bd _ _ ~ sd _"` extends `bd` then has a rest then extends `sd`.
    elongation: $ => '_',

    // [a b c] groups subdivide one step into the inner pattern. The
    // alternation form `[a|b|c]` chooses one of the inner patterns at
    // random per the docs (`s "[bd|hh|cp]"`); we model `,` as a
    // superposition operator (`s "[bd*2,hh*3]"`).
    group: $ => seq(
      '[',
      $._pattern_list,
      ']',
    ),

    // <a b c> alternation cycles through the inner steps one per cycle
    // (per the docs: `s "bd <sd hh cp>"`).
    alternation: $ => seq(
      '<',
      $._pattern_list,
      '>',
    ),

    // {a b c} polymetric, optionally followed by `%N` to override the
    // subdivision rate (per the docs: `s "{bd hh}%8"`). The trailing
    // `%N` is required to be immediately adjacent so `{bd hh}%8`
    // parses as one polymetric and `{bd hh} %8` doesn't.
    polymetric: $ => seq(
      '{',
      $._pattern_list,
      '}',
      optional(seq(
        token.immediate('%'),
        field('subdivision', alias(token.immediate(/[0-9]+(\.[0-9]+)?/), $.number)),
      )),
    ),

    // Inside any container, `,` separates parallel layers (superposition)
    // and `|` separates random alternatives. The docs only show one
    // separator class per container in their examples, but the grammar
    // permits either; the consumer can typecheck shape downstream.
    _pattern_list: $ => seq(
      $._pattern,
      repeat(seq(
        choice(',', '|'),
        $._pattern,
      )),
    ),

    // Suffixes per the docs. Prefer the longer matches first so e.g.
    // `?0.8` parses as one probability suffix rather than `?` + `0.8`
    // as a step.
    _suffix: $ => choice(
      $.repeat_suffix,
      $.divide_suffix,
      $.elongate_suffix,
      $.replicate_suffix,
      $.probability_suffix,
      $.ratio_suffix,
      $.euclid_suffix,
    ),

    // Suffixes are written immediately adjacent to the step they
    // modify in the corpus examples (`bd*2`, `bd!3`, `?0.8`); `token.immediate`
    // enforces no-whitespace between the operator and its trailing
    // number, which also resolves the otherwise-ambiguous `!` parse.

    // `*N` step repetition (`s "bd*2 sd"`).
    repeat_suffix: $ => seq('*', alias(token.immediate(/[0-9]+(\.[0-9]+)?/), $.number)),

    // `/N` step division (`s "bd/2"`).
    divide_suffix: $ => seq('/', alias(token.immediate(/[0-9]+(\.[0-9]+)?/), $.number)),

    // `@N` elongation by N (`s "superpiano@3 superpiano"`).
    elongate_suffix: $ => seq('@', alias(token.immediate(/[0-9]+(\.[0-9]+)?/), $.number)),

    // `!` or `!N` replication (`s "bd!3 sd"`). The optional immediate
    // number disambiguates `bd!3` (replicate-by-3) from `bd! 3`
    // (replicate-default then literal `3`).
    replicate_suffix: $ => seq('!', optional(alias(token.immediate(/[0-9]+(\.[0-9]+)?/), $.number))),

    // `?` or `?N.NN` probabilistic removal. The docs show both bare `?`
    // (default 0.5) and explicit form `?0.8`.
    probability_suffix: $ => seq('?', optional(alias(token.immediate(/[0-9]+(\.[0-9]+)?/), $.number))),

    // `%N` numeric ratio (rare; per the docs `"bd*4%2"`).
    ratio_suffix: $ => seq('%', alias(token.immediate(/[0-9]+(\.[0-9]+)?/), $.number)),

    // `(B,S)` or `(B,S,O)` euclidean (`s "bd(3,8)"`, `(3,8,1)`).
    euclid_suffix: $ => seq(
      '(',
      field('beats', $.number),
      ',',
      field('steps', $.number),
      optional(seq(',', field('offset', $.number))),
      ')',
    ),

    // Identifiers per the corpus examples: alphanumeric plus underscore,
    // starting with a letter. The docs do not formally specify the
    // identifier alphabet; this matches every documented sample name
    // (`bd`, `cp`, `hh`, `sd`, `superpiano`, `arpy`, `stab`, `cr`).
    identifier: $ => /[a-zA-Z][a-zA-Z0-9_]*/,

    // Numbers cover both integer and decimal forms; the docs use plain
    // integers for `:N`, `*N`, `/N`, `!N`, `@N`, ratios, and Euclidean,
    // and decimal fractions for `?0.8`. Single regex covers both.
    number: $ => /[0-9]+(\.[0-9]+)?/,
  },
});

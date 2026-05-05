; Comments
(comment) @comment

; Literals
(int_literal) @number
(float_literal) @number.float
(string_literal) @string
(char_literal) @character
(escape_sequence) @string.escape

; Special literals
(complex_literal) @number
(polar_literal) @number
(vec_literal) @number
(nil_literal) @constant.builtin

; Type declarations - mark types
(type_decl
  (type_decl_a
    (id_dot
      (identifier) @type)))

; Built-in types
((identifier) @type.builtin
  (#any-of? @type.builtin
    "int" "float" "dur" "time" "void" "complex" "polar"
    "vec2" "vec3" "vec4" "string" "Object" "Event" "Type"))

; UGen types (common audio unit generators)
((identifier) @type.builtin
  (#any-of? @type.builtin
    "SinOsc" "SqrOsc" "SawOsc" "TriOsc" "PulseOsc" "Phasor"
    "Noise" "Impulse" "Step" "Gain" "Pan2" "Mix2"
    "dac" "adc" "blackhole"
    "LPF" "HPF" "BPF" "BRF" "ResonZ" "BiQuad"
    "OnePole" "TwoPole" "OneZero" "TwoZero" "PoleZero"
    "ADSR" "Envelope" "Delay" "DelayL" "DelayA" "Echo"
    "JCRev" "NRev" "PRCRev" "Chorus" "PitShift" "Dyno"
    "SndBuf" "SndBuf2" "LiSa" "WvIn" "WvOut"
    "FFT" "IFFT" "DCT" "IDCT"
    "Mandolin" "Moog" "Rhodey" "Wurley" "Shakers"
    "FM" "BeeThree" "HevyMetl" "PercFlut" "TubeBell"))

; Built-in variables
((identifier) @variable.builtin
  (#any-of? @variable.builtin "now" "me" "null" "NULL" "true" "false" "pi"))

; Standard library
((identifier) @variable.builtin
  (#any-of? @variable.builtin "Math" "Std" "Machine" "CKDoc" "IO"))

; Function definitions
(function_definition
  name: (identifier) @function)

; Function declarations (fun, function, public, etc.)
(function_decl) @keyword.function

; Static/abstract modifiers (static, pure)
(static_decl) @keyword.modifier

; Class definitions
(class_definition
  name: (id_list
    (identifier) @type.definition))

; Class declaration keywords (public, private)
(class_decl) @keyword.modifier

; Class/interface keywords
["class" "interface"] @keyword.type

; Inheritance keywords
["extends" "implements"] @keyword.type

; Inherited types
(class_ext
  (id_dot
    (identifier) @type))

; Function calls
(postfix_expression
  function: (postfix_expression
    (identifier) @function.call))

; Method calls - member access where it's followed by call
(postfix_expression
  object: (postfix_expression)
  member: (identifier) @function.method)

; Parameters
(parameter
  (var_decl
    name: (identifier) @variable.parameter))

; Variable declarations
(var_decl
  name: (identifier) @variable)

; Chuck operators (=>, @=>, etc.)
(chuck_operator) @operator

; Arrow operators (->, <-, etc.)
(arrow_operator) @operator

; Overloadable operators in function definitions
(overloadable_operator) @operator

; Control flow keywords
["if" "else" "while" "until" "for" "do" "repeat"] @keyword.control

; Jump statements
["return" "break" "continue"] @keyword.control.return

; New/typeof/sizeof keywords
["new" "typeof" "sizeof"] @keyword.operator

; Spork keyword
"spork" @keyword.coroutine

; Annotation keywords
"@construct" @attribute
"@destruct" @attribute
"@operator" @attribute
"@import" @attribute
"@doc" @attribute

; Punctuation
["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["," ";" "."] @punctuation.delimiter
["<<<" ">>>"] @punctuation.special

; Hack expression delimiters
(hack_expression
  "<<<" @punctuation.special
  ">>>" @punctuation.special)

; General identifier fallback (must be last)
(identifier) @variable

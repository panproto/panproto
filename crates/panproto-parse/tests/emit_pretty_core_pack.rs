//! Extensive roundtrip tests for every core-pack language.
//!
//! Each test parses a representative source snippet, strips byte-position
//! constraints, emits via emit_pretty, re-parses, and asserts the vertex-kind
//! multiset is preserved. This exercises diverse grammatical constructs per
//! language to ensure the emission machinery generalizes.

#![cfg(feature = "grammars")]
#![allow(dead_code, clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;

use panproto_parse::ParserRegistry;
use panproto_schema::Schema;

fn registry() -> ParserRegistry {
    ParserRegistry::new()
}

fn strip_byte_fragments(schema: &mut Schema) {
    for constraints in schema.constraints.values_mut() {
        constraints.retain(|c| {
            let s = c.sort.as_ref();
            !(s == "start-byte" || s == "end-byte" || s.starts_with("interstitial-"))
        });
    }
}

fn vertex_kind_multiset(schema: &Schema) -> BTreeMap<String, usize> {
    let mut map = BTreeMap::new();
    for v in schema.vertices.values() {
        *map.entry(v.kind.to_string()).or_insert(0) += 1;
    }
    map
}

fn with_big_stack<F: FnOnce() + Send + 'static>(inner: F) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(inner)
        .expect("spawn")
        .join()
        .expect("worker panicked");
}

fn round_trip_inner(protocol: &str, source: &[u8]) {
    let registry = registry();
    let mut schema =
        match registry.parse_with_protocol(protocol, source, &format!("test.{protocol}")) {
            Ok(s) => s,
            Err(panproto_parse::ParseError::UnknownLanguage { .. }) => {
                eprintln!("skipping {protocol}: grammar not registered");
                return;
            }
            Err(e) => panic!("parse failed for {protocol}: {e}"),
        };
    strip_byte_fragments(&mut schema);
    let emitted = registry
        .emit_pretty_with_protocol(protocol, &schema)
        .unwrap_or_else(|e| panic!("emit_pretty failed for {protocol}: {e}"));
    assert!(!emitted.is_empty(), "empty emit for {protocol}");
    let reparsed = registry
        .parse_with_protocol(protocol, &emitted, &format!("rt.{protocol}"))
        .unwrap_or_else(|e| {
            let preview = std::str::from_utf8(&emitted).unwrap_or("<non-utf8>");
            panic!("reparse failed for {protocol}: {e}\nemitted:\n{preview}")
        });
    let orig = vertex_kind_multiset(&schema);
    let re = vertex_kind_multiset(&reparsed);
    if orig != re {
        let preview = std::str::from_utf8(&emitted).unwrap_or("<non-utf8>");
        panic!("multiset diverged for {protocol}\nemitted:\n{preview}\norig: {orig:?}\nre: {re:?}");
    }
}

fn round_trip(protocol: &'static str, source: &'static [u8]) {
    with_big_stack(move || round_trip_inner(protocol, source));
}

// ═══════════════════════════════════════════════════════════════
// Python
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "lang-python")]
mod python {
    use super::*;

    #[test]
    fn class_with_methods() {
        round_trip(
            "python",
            b"class Foo:\n    def bar(self):\n        return 1\n",
        );
    }

    #[test]
    fn list_comprehension() {
        round_trip("python", b"xs = [x * 2 for x in range(10)]\n");
    }

    #[test]
    fn decorator() {
        round_trip("python", b"@staticmethod\ndef f():\n    pass\n");
    }

    #[test]
    fn with_statement() {
        round_trip("python", b"with open('f') as fp:\n    data = fp.read()\n");
    }

    #[test]
    fn import_from() {
        round_trip("python", b"from os.path import join\n");
    }

    #[test]
    fn if_elif_else() {
        round_trip(
            "python",
            b"if x > 0:\n    y = 1\nelif x < 0:\n    y = -1\nelse:\n    y = 0\n",
        );
    }

    #[test]
    fn try_except() {
        round_trip("python", b"try:\n    f()\nexcept ValueError:\n    pass\n");
    }

    #[test]
    fn lambda() {
        round_trip("python", b"f = lambda x, y: x + y\n");
    }

    #[test]
    fn dict_literal() {
        round_trip("python", b"d = {'a': 1, 'b': 2}\n");
    }

    #[test]
    fn tuple_unpacking() {
        round_trip("python", b"a, b = 1, 2\n");
    }
}

// ═══════════════════════════════════════════════════════════════
// JavaScript
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "lang-javascript")]
mod javascript {
    use super::*;

    #[test]
    fn class_with_constructor() {
        round_trip(
            "javascript",
            b"class Foo {\n  constructor(x) {\n    this.x = x;\n  }\n}\n",
        );
    }

    #[test]
    fn for_of_loop() {
        round_trip("javascript", b"for (const x of items) {\n  f(x);\n}\n");
    }

    #[test]
    fn async_await() {
        round_trip(
            "javascript",
            b"async function fetch() {\n  const r = await get();\n  return r;\n}\n",
        );
    }

    #[test]
    fn destructuring_assignment() {
        round_trip("javascript", b"const {a, b} = obj;\n");
    }

    #[test]
    fn ternary_expression() {
        round_trip("javascript", b"var x = a > 0 ? a : -a;\n");
    }

    #[test]
    fn array_methods() {
        round_trip("javascript", b"var ys = xs.map(x => x + 1);\n");
    }

    #[test]
    fn switch_case() {
        round_trip(
            "javascript",
            b"switch (x) {\n  case 1:\n    break;\n  default:\n    break;\n}\n",
        );
    }

    #[test]
    #[ignore = "EMITTER: regex / delimiters are STRING tokens in SEQ but get Operator spacing; IMMEDIATE_TOKEN closing / not tight"]
    fn regex_literal() {
        round_trip("javascript", b"var re = /abc/g;\n");
    }

    #[test]
    fn try_catch_finally() {
        round_trip(
            "javascript",
            b"try {\n  f();\n} catch (e) {\n  g(e);\n} finally {\n  h();\n}\n",
        );
    }
}

// ═══════════════════════════════════════════════════════════════
// TypeScript
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "lang-typescript")]
mod typescript {
    use super::*;

    #[test]
    fn interface_declaration() {
        round_trip(
            "typescript",
            b"interface Point {\n  x: number;\n  y: number;\n}\n",
        );
    }

    #[test]
    fn generic_function() {
        round_trip("typescript", b"function id<T>(x: T): T {\n  return x;\n}\n");
    }

    #[test]
    fn type_alias() {
        round_trip("typescript", b"type Pair<A, B> = [A, B];\n");
    }

    #[test]
    fn enum_declaration() {
        round_trip(
            "typescript",
            b"enum Color {\n  Red,\n  Green,\n  Blue,\n}\n",
        );
    }

    #[test]
    fn optional_chaining() {
        round_trip("typescript", b"const x = a?.b?.c;\n");
    }
}

// ═══════════════════════════════════════════════════════════════
// Java
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "lang-java")]
mod java {
    use super::*;

    #[test]
    fn interface_with_method() {
        round_trip("java", b"interface Runnable {\n  void run();\n}\n");
    }

    #[test]
    fn generic_class() {
        round_trip(
            "java",
            b"class Box<T> {\n  T value;\n  T get() {\n    return value;\n  }\n}\n",
        );
    }

    #[test]
    #[ignore = "EMITTER: marker_annotation (@Override) child not consumed; modifiers CHOICE dispatch fails"]
    fn annotation() {
        round_trip(
            "java",
            b"class Foo {\n  @Override\n  public String toString() {\n    return \"Foo\";\n  }\n}\n",
        );
    }

    #[test]
    fn enum_declaration() {
        round_trip("java", b"enum Color {\n  RED,\n  GREEN,\n  BLUE\n}\n");
    }

    #[test]
    fn for_each_loop() {
        round_trip(
            "java",
            b"class Main {\n  void f(int[] xs) {\n    for (int x : xs) {\n      g(x);\n    }\n  }\n}\n",
        );
    }

    #[test]
    fn try_with_resources() {
        round_trip(
            "java",
            b"class Main {\n  void f() throws Exception {\n    try {\n      g();\n    } catch (Exception e) {\n      h();\n    }\n  }\n}\n",
        );
    }
}

// ═══════════════════════════════════════════════════════════════
// C#
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "lang-csharp")]
mod csharp {
    use super::*;

    #[test]
    fn property_declaration() {
        round_trip("csharp", b"class Foo {\n  public int X { get; set; }\n}\n");
    }

    #[test]
    fn generic_method() {
        round_trip(
            "csharp",
            b"class Foo {\n  T Id<T>(T x) {\n    return x;\n  }\n}\n",
        );
    }

    #[test]
    fn using_statement() {
        round_trip(
            "csharp",
            b"class Foo {\n  void F() {\n    using (var s = new S()) {\n      s.Do();\n    }\n  }\n}\n",
        );
    }
}

// ═══════════════════════════════════════════════════════════════
// C++
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "lang-cpp")]
mod cpp {
    use super::*;

    #[test]
    fn template_function() {
        round_trip(
            "cpp",
            b"template<typename T>\nT id(T x) {\n  return x;\n}\n",
        );
    }

    #[test]
    fn namespace_and_class() {
        round_trip(
            "cpp",
            b"namespace ns {\n  class Foo {\n  public:\n    int x;\n  };\n}\n",
        );
    }

    #[test]
    fn reference_and_pointer() {
        round_trip("cpp", b"int& ref(int& x) {\n  return x;\n}\n");
    }

    #[test]
    #[ignore = "EMITTER: _for_statement_body hidden rule FIELD children (initializer, condition, update) not consumed by CHOICE dispatch"]
    fn range_for() {
        round_trip(
            "cpp",
            b"void f(int* xs, int n) {\n  for (int i = 0; i < n; i++) {\n    g(xs[i]);\n  }\n}\n",
        );
    }
}

// ═══════════════════════════════════════════════════════════════
// C
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "lang-c")]
mod c {
    use super::*;

    #[test]
    fn struct_definition() {
        round_trip("c", b"struct Point {\n  int x;\n  int y;\n};\n");
    }

    #[test]
    fn pointer_arithmetic() {
        round_trip("c", b"void f(int* p, int n) {\n  int* end = p + n;\n}\n");
    }

    #[test]
    fn if_else() {
        round_trip(
            "c",
            b"int abs(int x) {\n  if (x < 0) {\n    return -x;\n  } else {\n    return x;\n  }\n}\n",
        );
    }

    #[test]
    fn typedef() {
        round_trip("c", b"typedef unsigned long size_t;\n");
    }

    #[test]
    fn switch_statement() {
        round_trip(
            "c",
            b"int f(int x) {\n  switch (x) {\n    case 0:\n      return 1;\n    default:\n      return 0;\n  }\n}\n",
        );
    }
}

// ═══════════════════════════════════════════════════════════════
// Go
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "lang-go")]
mod go {
    use super::*;

    #[test]
    fn interface_declaration() {
        round_trip(
            "go",
            b"package main\n\ntype Reader interface {\n\tRead(p []byte) (int, error)\n}\n",
        );
    }

    #[test]
    fn struct_with_methods() {
        round_trip(
            "go",
            b"package main\n\ntype Point struct {\n\tX int\n\tY int\n}\n\nfunc (p Point) Sum() int {\n\treturn p.X + p.Y\n}\n",
        );
    }

    #[test]
    fn goroutine_and_channel() {
        round_trip(
            "go",
            b"package main\n\nfunc f() {\n\tch := make(chan int)\n\tgo func() {\n\t\tch <- 1\n\t}()\n}\n",
        );
    }

    #[test]
    fn for_range() {
        round_trip(
            "go",
            b"package main\n\nfunc f(xs []int) int {\n\ts := 0\n\tfor _, x := range xs {\n\t\ts += x\n\t}\n\treturn s\n}\n",
        );
    }

    #[test]
    #[ignore = "EMITTER: Go if-with-init CHOICE[SEQ[init,;],BLANK] selects non-BLANK despite no initializer FIELD edge"]
    fn if_with_init() {
        round_trip(
            "go",
            b"package main\n\nfunc f() {\n\tif x := g(); x > 0 {\n\t\th(x)\n\t}\n}\n",
        );
    }
}

// ═══════════════════════════════════════════════════════════════
// Rust
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "lang-rust")]
mod rust {
    use super::*;

    #[test]
    fn trait_definition() {
        round_trip(
            "rust",
            b"trait Greet {\n    fn hello(&self) -> String;\n}\n",
        );
    }

    #[test]
    #[ignore = "EMITTER: enum_variant REPEAT body children lost; CHOICE inside enum_variant_list fails to consume variant edges"]
    fn enum_with_variants() {
        round_trip("rust", b"enum Option<T> {\n    Some(T),\n    None,\n}\n");
    }

    #[test]
    fn closure() {
        round_trip("rust", b"fn f() {\n    let add = |a, b| a + b;\n}\n");
    }

    #[test]
    #[ignore = "WALKER: tail expression wrapped as expression_statement adds ; not in source; re-parse creates extra empty_statement"]
    fn match_expression() {
        round_trip(
            "rust",
            b"fn f(x: i32) -> i32 {\n    match x {\n        0 => 1,\n        _ => x,\n    }\n}\n",
        );
    }

    #[test]
    fn impl_block() {
        round_trip(
            "rust",
            b"struct Foo;\n\nimpl Foo {\n    fn new() -> Self {\n        Foo\n    }\n}\n",
        );
    }

    #[test]
    fn lifetime_annotation() {
        round_trip("rust", b"fn first<'a>(s: &'a str) -> &'a str {\n    s\n}\n");
    }

    #[test]
    fn use_statement() {
        round_trip("rust", b"use std::collections::HashMap;\n");
    }
}

// ═══════════════════════════════════════════════════════════════
// PHP
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "lang-php")]
mod php {
    use super::*;

    #[test]
    fn class_with_method() {
        round_trip(
            "php",
            b"<?php\nclass Foo {\n  public function bar() {\n    return 1;\n  }\n}\n",
        );
    }

    #[test]
    fn array_literal() {
        round_trip("php", b"<?php\n$a = [1, 2, 3];\n");
    }

    #[test]
    fn foreach_loop() {
        round_trip(
            "php",
            b"<?php\nforeach ($items as $key => $value) {\n  echo $key;\n}\n",
        );
    }

    #[test]
    fn namespace_and_use() {
        round_trip("php", b"<?php\nnamespace App;\nuse App\\Model;\n");
    }
}

// ═══════════════════════════════════════════════════════════════
// Bash
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "lang-bash")]
mod bash {
    use super::*;

    #[test]
    fn if_then_fi() {
        round_trip("bash", b"if [ -f foo ]; then\n  echo found\nfi\n");
    }

    #[test]
    #[ignore = "EMITTER: case_item REPEAT body children not consumed; CHOICE dispatch for ;; and ) delimiters"]
    fn case_esac() {
        round_trip(
            "bash",
            b"case $x in\n  a)\n    echo a\n    ;;\n  *)\n    echo other\n    ;;\nesac\n",
        );
    }

    #[test]
    fn pipe() {
        round_trip("bash", b"cat file | grep foo | wc -l\n");
    }

    #[test]
    fn for_loop() {
        round_trip("bash", b"for i in 1 2 3; do\n  echo $i\ndone\n");
    }

    #[test]
    #[ignore = "EMITTER: = in CHOICE[\"=\",\"+=\"] gets Operator role instead of Connector; unwrap_to_string does not handle CHOICE-of-strings"]
    fn variable_assignment() {
        round_trip("bash", b"x=hello\necho $x\n");
    }
}

// ═══════════════════════════════════════════════════════════════
// Julia (beyond base roundtrip)
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "lang-julia")]
mod julia {
    use super::*;

    #[test]
    fn struct_definition() {
        round_trip("julia", b"struct Point\n    x\n    y\nend\n");
    }

    #[test]
    #[ignore = "EMITTER: elseif_clause and else_clause FIELD children need LineBreak before them inside word-like bracket pair"]
    fn if_elseif_else() {
        round_trip(
            "julia",
            b"if x > 0\n    1\nelseif x < 0\n    -1\nelse\n    0\nend\n",
        );
    }

    #[test]
    fn for_loop() {
        round_trip("julia", b"for i in 1:10\n    println(i)\nend\n");
    }

    #[test]
    fn while_loop() {
        round_trip("julia", b"while x > 0\n    x = x - 1\nend\n");
    }

    #[test]
    #[ignore = "EMITTER: module_definition body CHOICE[block,BLANK] has no body-position LineBreak detection (block through FIELD wrapping)"]
    fn module_definition() {
        round_trip("julia", b"module Foo\n    export bar\n    bar() = 1\nend\n");
    }

    #[test]
    fn anonymous_function() {
        round_trip("julia", b"f = x -> x + 1\n");
    }

    #[test]
    fn array_comprehension() {
        round_trip("julia", b"xs = [x^2 for x in 1:10]\n");
    }
}

// ═══════════════════════════════════════════════════════════════
// Stan
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "lang-stan")]
mod stan {
    use super::*;

    #[test]
    #[ignore = "PARSER: original parse has ERROR:1 from y[N] array syntax; emitter drops ERROR content, re-parse diverges"]
    fn full_model() {
        round_trip(
            "stan",
            b"data {\n  int N;\n  real y[N];\n}\nparameters {\n  real mu;\n  real<lower=0> sigma;\n}\nmodel {\n  mu ~ normal(0, 10);\n  y ~ normal(mu, sigma);\n}\n",
        );
    }

    #[test]
    fn transformed_data() {
        round_trip(
            "stan",
            b"data {\n  int N;\n}\ntransformed data {\n  int M;\n  M = N * 2;\n}\nmodel {}\n",
        );
    }
}

// ═══════════════════════════════════════════════════════════════
// BUGS
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "lang-bugs")]
mod bugs {
    use super::*;

    #[test]
    fn hierarchical_model() {
        round_trip(
            "bugs",
            b"model {\n  for (i in 1:N) {\n    y[i] ~ dnorm(mu, tau)\n  }\n  mu ~ dnorm(0, 0.001)\n  tau ~ dgamma(0.01, 0.01)\n}\n",
        );
    }
}

// ═══════════════════════════════════════════════════════════════
// JAGS
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "lang-jags")]
mod jags {
    use super::*;

    #[test]
    fn model_with_data() {
        round_trip(
            "jags",
            b"data {\n  for (i in 1:N) {\n    y[i] ~ dnorm(0, 1)\n  }\n}\nmodel {\n  mu ~ dnorm(0, 0.001)\n}\n",
        );
    }
}

// ═══════════════════════════════════════════════════════════════
// Scheme
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "lang-scheme")]
mod scheme {
    use super::*;

    #[test]
    fn let_binding() {
        round_trip("scheme", b"(let ((x 1) (y 2)) (+ x y))\n");
    }

    #[test]
    fn cond_expression() {
        round_trip("scheme", b"(cond ((> x 0) 1) ((< x 0) -1) (else 0))\n");
    }

    #[test]
    fn lambda_and_map() {
        round_trip("scheme", b"(map (lambda (x) (* x x)) (list 1 2 3))\n");
    }
}

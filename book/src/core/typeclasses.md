# Typeclasses as theory morphisms

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

A Haskell programmer reads `Eq a => a -> a -> Bool` and sees a familiar story: a type `a` is in the class `Eq` if it supplies an `==` operation satisfying the class's axioms, and a function that demands `Eq a` is one whose body uses `==`. Typeclasses are the mechanism by which Haskell attaches a set of operations, and their laws, to a type; an instance is the evidence that a given type supports those operations.

Typeclasses are not a new mathematical primitive. They are a notational convenience for something we already have: a typeclass is a theory, an instance is a morphism from that theory into a target theory, and the instance's bindings are the morphism's action on the class's operations. This chapter says what that correspondence looks like in panproto, and how `panproto-theory-dsl` and `panproto-gat-macros` give it a working surface.

## The correspondence

Take Haskell's `Eq` class. It declares one operation, `==`, of type `a -> a -> Bool`, together with the reflexivity, symmetry, and transitivity axioms. Written as a theory:

```text
theory ThEq {
    sorts  { A; Bool }
    ops    { eq : (A, A) -> Bool }
    axioms { refl: eq(x, x) = true }
}
```

An instance `instance Eq Int where x == y = primIntEq x y` says: take the class theory `ThEq`, pick the target theory where the primitive `primIntEq` lives (a theory of arithmetic), and declare a morphism that sends the class's sort `A` to `Int` and the class's operation `eq` to `primIntEq`. The morphism obligation is the axiom: `primIntEq(x, x) = true` has to hold in the target theory, which is how panproto's typechecker verifies the instance.

The correspondence extends uniformly:

| Haskell                          | panproto                                                       |
|----------------------------------|----------------------------------------------------------------|
| `class C a where ...`            | `Theory` declaring the class's sorts, ops, and axioms          |
| `instance C T where op = expr`   | `TheoryMorphism` from the class theory to a target theory      |
| `C a => ...` constraint          | A precondition that the morphism from `C`'s theory exists      |
| Class axiom                      | Equation in the class theory, required to hold under morphism  |
| Default method                   | Equation in the class theory giving a derived-op definition    |
| Functional dependency `a -> b`   | Sort parameter `b` implicitly determined by `a` in the class   |

The Haskell-to-panproto translation is always the same shape: the class is a theory with a distinguished sort the instance will replace, the instance is a morphism that picks the replacement and supplies implementations for every op the class declares.

## Two surfaces

Panproto exposes two authoring surfaces for this correspondence, at opposite ends of the config-versus-code spectrum, both compiling to the same [`Theory`](https://docs.rs/panproto-gat/latest/panproto_gat/theory/struct.Theory.html) and [`TheoryMorphism`](https://docs.rs/panproto-gat/latest/panproto_gat/morphism/struct.TheoryMorphism.html) values.

The config surface is the [`panproto-theory-dsl`](https://docs.rs/panproto-theory-dsl/latest/panproto_theory_dsl/) crate, which parses a `TheoryDocument` from Nickel, JSON, or YAML and compiles it through the [`compile_class`](https://docs.rs/panproto-theory-dsl/latest/panproto_theory_dsl/compile_class/), [`compile_instance`](https://docs.rs/panproto-theory-dsl/latest/panproto_theory_dsl/compile_instance/), and [`compile_inductive`](https://docs.rs/panproto-theory-dsl/latest/panproto_theory_dsl/compile_inductive/) entry points. A class document declares the class's sorts, ops, and axioms; an instance document declares the source class, the target theory, and the bindings; the compiler resolves the target theory, builds the morphism, and returns a validated `TheoryMorphism`. This is the surface a user reaches for when theories live as data in a repository and are edited alongside the protocols they describe.

The code surface is the [`panproto-gat-macros`](https://docs.rs/panproto-gat-macros/latest/panproto_gat_macros/) crate, which offers three proc-macros. `class!` takes a class declaration in a Rust-ish syntax and expands to a `theory_<name>()` constructor returning the class's `Theory`. `instance!` takes an instance declaration and expands to an `instance_<name>(&class, &target)` function returning a `TheoryMorphism`. `inductive!` takes an inductive-type declaration and expands to the closed-sort `Theory` it names. The macros are the surface for users who prefer to author theories inline with the Rust code that consumes them.

```rust,ignore
use panproto_gat_macros::{class, instance};

class! {
    ThEq<A> {
        eq(x: A, y: A) -> Bool;
        axiom refl: eq(x, x) = true;
    }
}

instance! {
    EqInt: ThEq<Int> in ThArith {
        eq = int_eq;
    }
}
```

The choice between the two surfaces is a style question, not a feature question: both compile to the same validated `TheoryMorphism` and both go through the same typechecker. The fact that the two compile to identical values is what lets a theory author mix them in a single project; a core class in the DSL can have a downstream Rust crate declare an instance via the proc-macro, and the morphism composes with any other morphism in the system by the rules of [Theory morphisms and instance migration](./morphisms-and-migration.md).

## Deriving standard instances

Haskell's `deriving (Eq, Ord)` mechanism asks the compiler to synthesize instances of standard classes by structural recursion on the type's constructors. The panproto counterpart is `derive_theory!` in `panproto-gat-macros`, which accepts a theory-declaration block annotated with `#[derive(Eq)]` or `#[derive(Hash)]` and emits instance-builder functions for each derivation alongside the base theory. The two derivations cover the two capabilities every panproto sort is expected to carry (decidable equality and hashability), and are the common case that makes the `instance!` macro worth having. Extending the set of derivable classes is a deliberate, per-class piece of work; the 0.37 release supplies the two that every protocol needs and stops there.

## Why a morphism rather than a record of functions

A working engineer could ask why the machinery is a theory morphism rather than a record of function pointers. The answer is that the morphism carries the theory's equations along with its operations: the instance's validity is not only that each declared op has a matching target op, but that the target ops satisfy the class's axioms under the morphism. A record of function pointers is a data-only mapping; a theory morphism is a mapping that the engine can verify preserves the class's laws. The verification is what makes the instance trustworthy in the same way Haskell's axioms (informal as they are in the language proper) make a well-written `Eq` instance trustworthy. In panproto the axioms are part of the class theory, the instance is required to satisfy them under the morphism, and the typechecker rejects an instance whose bindings violate an axiom, with a diagnostic pointing at the specific axiom and specific binding.

## Further reading

Typeclasses as we know them in Haskell are due to Wadler and Blott, whose 1989 POPL paper introduced the ad-hoc-polymorphism-via-implicit-dictionaries design that the language adopted; see @wadler1989theorems for the antecedent work on parametricity that grounds the design. The categorical story of classes as theories and instances as morphisms is standard in the algebraic-specification tradition; @goguenburstall1992institutions is the institutions framework where the correspondence is stated most cleanly, and the modular-specification apparatus of the same line of research is what underlies the namespaced-imports machinery in `panproto-theory-dsl`.

## Closing

The typeclass surface is where the `panproto-gat` machinery meets the idiomatic shapes a working developer expects. A class is a theory, an instance is a morphism, and the two surfaces (DSL and proc-macros) are two ways to write the same objects. The next chapter, [The restrict/lift pipeline](./restrict-lift.md), turns back to the core migration construction and shows how a theory morphism drives instance migration end to end.

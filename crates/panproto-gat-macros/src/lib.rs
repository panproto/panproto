//! Proc-macro surface for declarative class and instance syntax
//! targeting panproto-gat.
//!
//! - `class! { ThEq<A> { ... } }` expands to a `theory_<lowercase>()`
//!   function that returns a `panproto_gat::Theory` built from the
//!   listed signatures and axioms.
//! - `instance! { Name: Class<Ty, ...> in Target { op = target_op; ... } }`
//!   expands to an `instance_<lowercase>(class, target)` function that
//!   builds a validated `panproto_gat::TheoryMorphism`.
//! - `inductive! { Nat { zero : Nat, succ(n: Nat) : Nat } }` expands to
//!   a theory constructor with a closed sort and one operation per
//!   listed constructor.
//! - `derive_theory!` accepts a theory-declaration block annotated
//!   with `#[derive(Eq)]` or `#[derive(Hash)]` and emits the base
//!   theory plus instance-builder functions for each derivation.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Ident, Token, braced, parenthesized, parse_macro_input};

// ═══════════════════════════════════════════════════════════════════
// class! macro
// ═══════════════════════════════════════════════════════════════════

/// AST for a single operation signature inside a class body.
struct SigItem {
    name: Ident,
    args: Punctuated<ArgItem, Token![,]>,
    output: SortExprAst,
}

struct ArgItem {
    name: Ident,
    ty: SortExprAst,
}

impl Parse for ArgItem {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty: SortExprAst = input.parse()?;
        Ok(Self { name, ty })
    }
}

// ─── Dependent sort + term ASTs ────────────────────────────────────────
//
// Surface grammar mirrors `panproto-theory-dsl`'s JSON / YAML / Nickel
// surface (see `crates/panproto-theory-dsl/tests/fixtures/stlc.json`):
//
//   SortExprAst := Ident                            -- e.g. `Ty`
//                | Ident `(` TermAst,* `)`          -- e.g. `Tm(arrow(a, b))`
//
//   TermAst     := Ident                            -- variable, e.g. `a`
//                | Ident `(` TermAst,* `)`          -- op application
//
// Both are parsed identically: the structural recursion is the same,
// only the head's role (sort name vs. op name) differs at codegen time.
// The macro does no name resolution — that is the GAT typechecker's
// job — so an erroneous reference produces a downstream
// `panproto-gat::GatError`, not a parse error. This matches the JSON
// surface, which also defers head validation to `compile_class`.

/// A sort expression appearing in argument or output position.
struct SortExprAst {
    head: Ident,
    args: Vec<TermAst>,
}

/// A term expression appearing inside a sort-expression argument list.
struct TermAst {
    head: Ident,
    args: Vec<Self>,
}

impl Parse for SortExprAst {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let head: Ident = input.parse()?;
        let args = if input.peek(syn::token::Paren) {
            let content;
            parenthesized!(content in input);
            let punc: Punctuated<TermAst, Token![,]> = Punctuated::parse_terminated(&content)?;
            punc.into_iter().collect()
        } else {
            Vec::new()
        };
        Ok(Self { head, args })
    }
}

impl Parse for TermAst {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let head: Ident = input.parse()?;
        let args = if input.peek(syn::token::Paren) {
            let content;
            parenthesized!(content in input);
            let punc: Punctuated<Self, Token![,]> = Punctuated::parse_terminated(&content)?;
            punc.into_iter().collect()
        } else {
            Vec::new()
        };
        Ok(Self { head, args })
    }
}

/// Emit a `panproto_gat::SortExpr` constructor for a [`SortExprAst`].
///
/// Uses the [`SortExpr::app`] smart constructor, which collapses the
/// no-args case to [`SortExpr::Name`] so the existing zero-argument
/// callers keep producing the same value.
fn sort_expr_to_tokens(expr: &SortExprAst) -> TokenStream2 {
    let head = expr.head.to_string();
    if expr.args.is_empty() {
        quote! { ::panproto_gat::SortExpr::Name(::std::sync::Arc::from(#head)) }
    } else {
        let arg_tokens = expr.args.iter().map(term_ast_to_tokens);
        quote! {
            ::panproto_gat::SortExpr::app(
                ::std::sync::Arc::<str>::from(#head),
                ::std::vec![ #( #arg_tokens ),* ],
            )
        }
    }
}

/// Emit a `panproto_gat::Term` constructor for a [`TermAst`].
///
/// A bare identifier emits [`Term::Var`]; an applied identifier emits
/// [`Term::App`]. The macro never produces [`Term::Case`], [`Term::Let`],
/// or [`Term::Hole`] — those constructs are not expressible in the
/// macro's surface and require the lower-level `panproto-gat` API.
fn term_ast_to_tokens(term: &TermAst) -> TokenStream2 {
    let head = term.head.to_string();
    if term.args.is_empty() {
        quote! { ::panproto_gat::Term::Var(::std::sync::Arc::from(#head)) }
    } else {
        let arg_tokens = term.args.iter().map(term_ast_to_tokens);
        quote! {
            ::panproto_gat::Term::App {
                op: ::std::sync::Arc::from(#head),
                args: ::std::vec![ #( #arg_tokens ),* ],
            }
        }
    }
}

/// AST for an equational axiom: `axiom name: lhs = rhs;`.
///
/// Both sides are parsed as terms from the tokens themselves, so each
/// identifier keeps the span it was written at and a malformed side is
/// reported by `syn` at the offending token.
struct AxiomItem {
    name: Ident,
    lhs: TermAst,
    rhs: TermAst,
}

enum ClassBodyItem {
    Sig(SigItem),
    Axiom(AxiomItem),
}

impl Parse for ClassBodyItem {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let lookahead = input.lookahead1();
        if lookahead.peek(Token![fn]) {
            return Err(input.error(
                "unexpected `fn`; use `name(arg: Sort, ...) -> Sort;` for class signatures",
            ));
        }
        // Peek for the keyword `axiom` (it is not a real Rust keyword, so
        // it parses as an Ident).
        let fork = input.fork();
        if let Ok(id) = fork.parse::<Ident>()
            && id == "axiom"
        {
            input.parse::<Ident>()?;
            let name: Ident = input.parse()?;
            input.parse::<Token![:]>()?;
            let lhs: TermAst = input.parse()?;
            input.parse::<Token![=]>()?;
            let rhs: TermAst = input.parse()?;
            input.parse::<Token![;]>()?;
            return Ok(Self::Axiom(AxiomItem { name, lhs, rhs }));
        }

        // Parse signature: name(args) -> Output;
        let name: Ident = input.parse()?;
        let content;
        parenthesized!(content in input);
        let args = Punctuated::<ArgItem, Token![,]>::parse_terminated(&content)?;
        input.parse::<Token![->]>()?;
        let output: SortExprAst = input.parse()?;
        input.parse::<Token![;]>()?;
        Ok(Self::Sig(SigItem { name, args, output }))
    }
}

struct ClassInput {
    name: Ident,
    params: Vec<Ident>,
    items: Vec<ClassBodyItem>,
}

impl Parse for ClassInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        if !input.peek(Token![<]) {
            return Err(
                input.error("class declaration requires a `<Param, ...>` sort-parameter list")
            );
        }
        input.parse::<Token![<]>()?;
        let params_punc: Punctuated<Ident, Token![,]> =
            Punctuated::parse_separated_nonempty(input)?;
        input.parse::<Token![>]>()?;
        let body;
        braced!(body in input);
        let mut items = Vec::new();
        while !body.is_empty() {
            items.push(body.parse::<ClassBodyItem>()?);
        }
        Ok(Self {
            name,
            params: params_punc.into_iter().collect(),
            items,
        })
    }
}

/// Expand to a `pub fn theory_<lower>() -> panproto_gat::Theory` that
/// builds the theory.
///
/// # Examples
///
/// ```
/// use panproto_gat_macros::class;
///
/// class! {
///     ThEq<A, Bool> {
///         eq(x: A, y: A) -> Bool;
///         neq(x: A, y: A) -> Bool;
///
///         axiom sym: eq(x, y) = eq(y, x);
///     }
/// }
///
/// let theory = theory_theq();
/// assert_eq!(&*theory.name, "ThEq");
/// assert_eq!(theory.ops.len(), 2);
/// assert_eq!(theory.eqs.len(), 1);
/// ```
#[proc_macro]
pub fn class(input: TokenStream) -> TokenStream {
    let class = parse_macro_input!(input as ClassInput);
    match class_theory_tokens(&class, "class!") {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Build the theory-constructor function for a parsed class body.
///
/// Both `class!` and `derive_theory!` expand their body through this, so a
/// given body yields the same sorts, operations and equations whichever macro
/// introduces it. `macro_name` names the macro in the generated function's doc
/// comment.
fn class_theory_tokens(class: &ClassInput, macro_name: &str) -> syn::Result<TokenStream2> {
    let name_str = class.name.to_string();
    let fn_name = format_ident!("theory_{}", name_str.to_lowercase());

    let sort_names: Vec<String> = class.params.iter().map(ToString::to_string).collect();
    let sort_inits = sort_names.iter().map(|n| {
        quote! { ::panproto_gat::Sort::simple(#n) }
    });

    let mut op_inits: Vec<TokenStream2> = Vec::new();
    let mut eq_inits: Vec<TokenStream2> = Vec::new();

    for item in &class.items {
        match item {
            ClassBodyItem::Sig(SigItem {
                name: op_name,
                args,
                output,
            }) => {
                let op_name_str = op_name.to_string();
                let output_tokens = sort_expr_to_tokens(output);
                let arg_triples = args.iter().map(|ArgItem { name, ty }| {
                    let n = name.to_string();
                    let ty_tokens = sort_expr_to_tokens(ty);
                    quote! {
                        (
                            ::std::sync::Arc::from(#n),
                            #ty_tokens,
                            ::panproto_gat::Implicit::No,
                        )
                    }
                });
                op_inits.push(quote! {
                    ::panproto_gat::Operation::with_implicit(
                        #op_name_str,
                        ::std::vec![ #( #arg_triples ),* ],
                        #output_tokens,
                    )
                });
            }
            ClassBodyItem::Axiom(AxiomItem {
                name: ax_name,
                lhs,
                rhs,
            }) => {
                let ax_name_str = ax_name.to_string();
                let lhs_tokens = term_ast_to_tokens(&lhs);
                let rhs_tokens = term_ast_to_tokens(&rhs);
                eq_inits.push(quote! {
                    ::panproto_gat::Equation::new(
                        #ax_name_str,
                        #lhs_tokens,
                        #rhs_tokens,
                    )
                });
            }
        }
    }

    let doc = format!("Construct the `{name_str}` theory produced by the `{macro_name}` macro.");
    Ok(quote! {
        #[doc = #doc]
        pub fn #fn_name() -> ::panproto_gat::Theory {
            ::panproto_gat::Theory::new(
                #name_str,
                ::std::vec![ #( #sort_inits ),* ],
                ::std::vec![ #( #op_inits ),* ],
                ::std::vec![ #( #eq_inits ),* ],
            )
        }
    })
}

// ═══════════════════════════════════════════════════════════════════
// instance! macro
// ═══════════════════════════════════════════════════════════════════

struct InstanceBinding {
    from: Ident,
    to: Ident,
}

impl Parse for InstanceBinding {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let from: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        let to: Ident = input.parse()?;
        input.parse::<Token![;]>()?;
        Ok(Self { from, to })
    }
}

struct InstanceInput {
    name: Ident,
    class: Ident,
    type_args: Vec<Ident>,
    target: Ident,
    bindings: Vec<InstanceBinding>,
}

impl Parse for InstanceInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let class: Ident = input.parse()?;
        if !input.peek(Token![<]) {
            return Err(input.error(
                "instance declaration requires `ClassName<Type, ...>` after the instance name",
            ));
        }
        input.parse::<Token![<]>()?;
        let type_args_punc: Punctuated<Ident, Token![,]> =
            Punctuated::parse_separated_nonempty(input)?;
        input.parse::<Token![>]>()?;
        if !input.peek(Token![in]) {
            return Err(
                input.error("expected `in` between the class arguments and the target theory name")
            );
        }
        input.parse::<Token![in]>()?;
        let target: Ident = input.parse()?;
        let body;
        braced!(body in input);
        let mut bindings = Vec::new();
        while !body.is_empty() {
            bindings.push(body.parse::<InstanceBinding>()?);
        }
        Ok(Self {
            name,
            class,
            type_args: type_args_punc.into_iter().collect(),
            target,
            bindings,
        })
    }
}

/// Expand to a `pub fn instance_<lower>(class, target) -> Result<TheoryMorphism, GatError>`.
///
/// # Examples
///
/// ```no_run
/// use panproto_gat::Theory;
/// use panproto_gat_macros::{class, instance};
///
/// class! {
///     ThEq<A, Bool> {
///         eq(x: A, y: A) -> Bool;
///         neq(x: A, y: A) -> Bool;
///     }
/// }
///
/// instance! {
///     EqInt: ThEq<Int, Bool> in ThArith {
///         eq = int_eq;
///         neq = int_neq;
///     }
/// }
///
/// // `target` must supply the `int_eq` / `int_neq` operations the
/// // bindings map onto (build one with `Theory::new`).
/// # fn demo(target: &Theory) -> Result<(), panproto_gat::GatError> {
/// let class_theory = theory_theq();
/// let morphism = instance_eqint(&class_theory, target)?;
/// assert_eq!(&*morphism.domain, "ThEq");
/// # Ok(())
/// # }
/// ```
#[proc_macro]
pub fn instance(input: TokenStream) -> TokenStream {
    let InstanceInput {
        name,
        class,
        type_args,
        target,
        bindings,
    } = parse_macro_input!(input as InstanceInput);

    let name_str = name.to_string();
    let fn_name = format_ident!("instance_{}", name_str.to_lowercase());
    let class_str = class.to_string();
    let target_str = target.to_string();

    let type_arg_strs: Vec<String> = type_args.iter().map(ToString::to_string).collect();
    let binding_pairs = bindings.iter().map(|b| {
        let f = b.from.to_string();
        let t = b.to.to_string();
        quote! { (#f.to_string(), #t.to_string()) }
    });
    let type_arg_lits = type_arg_strs.iter().map(|s| quote! { #s.to_string() });

    let doc =
        format!("Construct the `{name_str}` instance morphism produced by the `instance!` macro.");
    let expanded = quote! {
        #[doc = #doc]
        pub fn #fn_name(
            class_theory: &::panproto_gat::Theory,
            target_theory: &::panproto_gat::Theory,
        ) -> ::std::result::Result<::panproto_gat::TheoryMorphism, ::panproto_gat::GatError> {
            let type_args: ::std::vec::Vec<::std::string::String> =
                ::std::vec![ #( #type_arg_lits ),* ];
            let bindings: ::std::vec::Vec<(::std::string::String, ::std::string::String)> =
                ::std::vec![ #( #binding_pairs ),* ];

            let mut sort_map: ::std::collections::HashMap<
                ::std::sync::Arc<str>, ::std::sync::Arc<str>
            > = ::std::collections::HashMap::new();
            let mut op_map: ::std::collections::HashMap<
                ::std::sync::Arc<str>, ::std::sync::Arc<str>
            > = ::std::collections::HashMap::new();

            // Pair each class sort param (in declaration order) with the
            // positionally-matching type argument from `Class<T1, ..>`.
            let class_sort_params: ::std::vec::Vec<::std::sync::Arc<str>> =
                class_theory.sorts.iter().map(|s| ::std::sync::Arc::clone(&s.name)).collect();
            if type_args.len() > class_sort_params.len() {
                return ::std::result::Result::Err(
                    ::panproto_gat::GatError::InstanceTypeArgsArity {
                        instance: ::std::string::String::from(#name_str),
                        class: ::std::string::String::from(#class_str),
                        passed: type_args.len(),
                        declared: class_sort_params.len(),
                    }
                );
            }
            for (param, arg) in class_sort_params.iter().zip(type_args.iter()) {
                sort_map.insert(::std::sync::Arc::clone(param), ::std::sync::Arc::from(arg.as_str()));
            }

            for (from, to) in &bindings {
                if class_theory.find_sort(from).is_some() {
                    sort_map.insert(
                        ::std::sync::Arc::from(from.as_str()),
                        ::std::sync::Arc::from(to.as_str()),
                    );
                } else if class_theory.find_op(from).is_some() {
                    op_map.insert(
                        ::std::sync::Arc::from(from.as_str()),
                        ::std::sync::Arc::from(to.as_str()),
                    );
                } else {
                    return ::std::result::Result::Err(
                        ::panproto_gat::GatError::InstanceBindingUnknown {
                            instance: ::std::string::String::from(#name_str),
                            class: ::std::string::String::from(#class_str),
                            name: from.clone(),
                        }
                    );
                }
            }

            let morphism = ::panproto_gat::TheoryMorphism::new(
                #name_str,
                #class_str,
                #target_str,
                sort_map,
                op_map,
            );
            ::panproto_gat::check_morphism(&morphism, class_theory, target_theory)?;
            ::std::result::Result::Ok(morphism)
        }
    };

    expanded.into()
}

// ═══════════════════════════════════════════════════════════════════
// derive_theory! macro
// ═══════════════════════════════════════════════════════════════════

struct DeriveTheoryInput {
    derives: Vec<Ident>,
    class: ClassInput,
}

impl Parse for DeriveTheoryInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        // Leading attribute of the form `#[derive(Eq, Hash)]`.
        input.parse::<Token![#]>()?;
        let bracket_content;
        syn::bracketed!(bracket_content in input);
        let derive_ident: Ident = bracket_content.parse()?;
        if derive_ident != "derive" {
            return Err(syn::Error::new(
                derive_ident.span(),
                "derive_theory! expects a `#[derive(...)]` attribute",
            ));
        }
        if !bracket_content.peek(syn::token::Paren) {
            return Err(syn::Error::new(
                derive_ident.span(),
                "derive_theory! expects `#[derive(...)]` with a parenthesized list of trait names",
            ));
        }
        let derive_list;
        parenthesized!(derive_list in bracket_content);
        let derives_punc: Punctuated<Ident, Token![,]> =
            Punctuated::parse_separated_nonempty(&derive_list)?;
        let class: ClassInput = input.parse()?;
        Ok(Self {
            derives: derives_punc.into_iter().collect(),
            class,
        })
    }
}

/// Build a theory together with auto-generated class instances.
///
/// # Examples
///
/// ```
/// use panproto_gat_macros::derive_theory;
///
/// derive_theory! {
///     #[derive(Eq)]
///     ThVertex<Vertex, Bool> {
///         name(x: Vertex) -> Bool;
///         id(x: Vertex) -> Vertex;
///
///         axiom id_absorbs: id(id(x)) = id(x);
///     }
/// }
///
/// // The theory builder is generated alongside one instance builder
/// // per listed derive (here, `instance_vertex_eq`).
/// let theory = theory_thvertex();
/// assert_eq!(&*theory.name, "ThVertex");
/// // The body's axioms are stated on the theory, as they are under `class!`.
/// assert_eq!(theory.eqs.len(), 1);
/// ```
///
/// Expands to the `class!`-style theory builder (`pub fn theory_thvertex()`)
/// plus one instance-builder function per listed derive. The body goes through
/// the same expansion `class!` uses, so its signatures and axioms produce the
/// same operations and equations either way.
///
/// Supported derives: `Eq`, `Hash`. Passing `Ord` or `Show` emits a
/// compile error directing callers to the follow-up work, which keeps
/// the surface stable for those derives when support lands.
#[proc_macro]
pub fn derive_theory(input: TokenStream) -> TokenStream {
    let DeriveTheoryInput { derives, class } = parse_macro_input!(input as DeriveTheoryInput);

    for d in &derives {
        let s = d.to_string();
        if !matches!(s.as_str(), "Eq" | "Hash") {
            return syn::Error::new(
                d.span(),
                format!("unknown derive target: {s} (supported: Eq, Hash)"),
            )
            .to_compile_error()
            .into();
        }
    }

    let class_name = class.name.clone();
    let class_tokens = match class_theory_tokens(&class, "derive_theory!") {
        Ok(tokens) => tokens,
        Err(e) => return e.to_compile_error().into(),
    };

    // Primary sort: the first param of the class.
    let Some(primary_sort) = class.params.first().cloned() else {
        return syn::Error::new(
            class_name.span(),
            "derive_theory expects at least one sort parameter",
        )
        .to_compile_error()
        .into();
    };

    let mut instance_fns: Vec<TokenStream2> = Vec::new();
    for d in &derives {
        let s = d.to_string();
        match s.as_str() {
            "Eq" => instance_fns.push(build_derived_instance("eq", &class_name, &primary_sort)),
            "Hash" => instance_fns.push(build_derived_instance("hash", &class_name, &primary_sort)),
            _ => {}
        }
    }

    let expanded = quote! {
        #class_tokens
        #( #instance_fns )*
    };
    expanded.into()
}

fn build_derived_instance(
    class_suffix: &str,
    theory_name: &Ident,
    primary_sort: &Ident,
) -> TokenStream2 {
    let theory_name_str = theory_name.to_string();
    let primary_sort_str = primary_sort.to_string();
    let lower = primary_sort_str.to_lowercase();
    let fn_name = format_ident!("instance_{lower}_{class_suffix}");
    let class_theory_name = format!("Th{}", capitalize(class_suffix));
    let op_name = class_suffix.to_string();
    let doc = format!(
        "Build a `{class_theory_name}` instance morphism from `{theory_name_str}` generated by `derive_theory!`."
    );
    quote! {
        #[doc = #doc]
        pub fn #fn_name(
            class_theory: &::panproto_gat::Theory,
            target_theory: &::panproto_gat::Theory,
        ) -> ::std::result::Result<::panproto_gat::TheoryMorphism, ::panproto_gat::GatError> {
            let mut sort_map: ::std::collections::HashMap<
                ::std::sync::Arc<str>, ::std::sync::Arc<str>
            > = ::std::collections::HashMap::new();
            let mut op_map: ::std::collections::HashMap<
                ::std::sync::Arc<str>, ::std::sync::Arc<str>
            > = ::std::collections::HashMap::new();
            // Map every class sort positionally onto the matching target
            // sort. Default dispatch: the primary class sort maps to the
            // target theory's primary sort (the first declared sort).
            let class_sort_params: ::std::vec::Vec<::std::sync::Arc<str>> =
                class_theory.sorts.iter().map(|s| ::std::sync::Arc::clone(&s.name)).collect();
            let target_sort_names: ::std::vec::Vec<::std::sync::Arc<str>> =
                target_theory.sorts.iter().map(|s| ::std::sync::Arc::clone(&s.name)).collect();
            if target_sort_names.is_empty() {
                return ::std::result::Result::Err(
                    ::panproto_gat::GatError::SortNotFound(#primary_sort_str.to_string())
                );
            }
            for (i, param) in class_sort_params.iter().enumerate() {
                let target = target_sort_names.get(i).cloned()
                    .unwrap_or_else(|| ::std::sync::Arc::clone(&target_sort_names[0]));
                sort_map.insert(::std::sync::Arc::clone(param), target);
            }
            // Map the class's primary operation onto a target op with
            // the same name when it exists; otherwise synthesise a
            // canonical default name by prefixing the primary sort.
            let default_op_name: ::std::sync::Arc<str> = ::std::sync::Arc::from(
                format!("{}_{}", #primary_sort_str.to_lowercase(), #op_name).as_str()
            );
            let resolved: ::std::sync::Arc<str> = if target_theory.find_op(&default_op_name).is_some() {
                default_op_name
            } else if target_theory.find_op(#op_name).is_some() {
                ::std::sync::Arc::from(#op_name)
            } else {
                return ::std::result::Result::Err(
                    ::panproto_gat::GatError::MissingOpMapping(#op_name.to_string())
                );
            };
            op_map.insert(::std::sync::Arc::from(#op_name), resolved);
            let morphism_name = format!("{}_{}_instance", #theory_name_str, #op_name);
            let morphism = ::panproto_gat::TheoryMorphism::new(
                morphism_name,
                class_theory.name.as_ref(),
                target_theory.name.as_ref(),
                sort_map,
                op_map,
            );
            ::panproto_gat::check_morphism(&morphism, class_theory, target_theory)?;
            ::std::result::Result::Ok(morphism)
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
}

// ═══════════════════════════════════════════════════════════════════
// inductive! macro
// ═══════════════════════════════════════════════════════════════════

/// AST for a single constructor inside an `inductive!` body.
///
/// Two surface forms are accepted:
///
/// ```text
/// zero : Nat,
/// succ(n: Nat) : Nat,
/// ```
struct InductiveCtor {
    name: Ident,
    inputs: Vec<ArgItem>,
    output: SortExprAst,
}

impl Parse for InductiveCtor {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        let inputs = if input.peek(syn::token::Paren) {
            let content;
            parenthesized!(content in input);
            let args = Punctuated::<ArgItem, Token![,]>::parse_terminated(&content)?;
            args.into_iter().collect()
        } else {
            Vec::new()
        };
        input.parse::<Token![:]>()?;
        let output: SortExprAst = input.parse()?;
        Ok(Self {
            name,
            inputs,
            output,
        })
    }
}

struct InductiveInput {
    name: Ident,
    ctors: Vec<InductiveCtor>,
}

impl Parse for InductiveInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        let body;
        braced!(body in input);
        let ctors_punc: Punctuated<InductiveCtor, Token![,]> = Punctuated::parse_terminated(&body)?;
        Ok(Self {
            name,
            ctors: ctors_punc.into_iter().collect(),
        })
    }
}

/// Build a closed inductive theory.
///
/// # Examples
///
/// ```
/// use panproto_gat_macros::inductive;
///
/// inductive! {
///     Nat {
///         zero : Nat,
///         succ(n: Nat) : Nat,
///     }
/// }
///
/// let theory = theory_nat();
/// assert_eq!(&*theory.name, "Nat");
/// assert!(theory.find_op("zero").is_some());
/// assert!(theory.find_op("succ").is_some());
/// ```
///
/// Expands to `pub fn theory_nat() -> panproto_gat::Theory` returning a
/// theory whose one sort is `Nat`, closed against `[zero, succ]`, with
/// one op per constructor.
#[proc_macro]
pub fn inductive(input: TokenStream) -> TokenStream {
    let InductiveInput { name, ctors } = parse_macro_input!(input as InductiveInput);
    let name_str = name.to_string();
    let fn_name = format_ident!("theory_{}", name_str.to_lowercase());

    let ctor_names: Vec<String> = ctors.iter().map(|c| c.name.to_string()).collect();
    let ctor_name_lits = ctor_names.iter().map(|n| quote! { #n });

    let sort_init = quote! {
        ::panproto_gat::Sort::closed(
            #name_str,
            ::std::vec::Vec::new(),
            ::std::vec![ #( #ctor_name_lits ),* ],
        )
    };

    let op_inits = ctors.iter().map(|c| {
        let op_name = c.name.to_string();
        let output_tokens = sort_expr_to_tokens(&c.output);
        let arg_triples = c.inputs.iter().map(|ArgItem { name, ty }| {
            let n = name.to_string();
            let ty_tokens = sort_expr_to_tokens(ty);
            quote! {
                (
                    ::std::sync::Arc::from(#n),
                    #ty_tokens,
                    ::panproto_gat::Implicit::No,
                )
            }
        });
        quote! {
            ::panproto_gat::Operation::with_implicit(
                #op_name,
                ::std::vec![ #( #arg_triples ),* ],
                #output_tokens,
            )
        }
    });

    let doc =
        format!("Construct the inductive theory `{name_str}` produced by the `inductive!` macro.");
    let expanded = quote! {
        #[doc = #doc]
        pub fn #fn_name() -> ::panproto_gat::Theory {
            ::panproto_gat::Theory::new(
                #name_str,
                ::std::vec![ #sort_init ],
                ::std::vec![ #( #op_inits ),* ],
                ::std::vec::Vec::new(),
            )
        }
    };
    expanded.into()
}

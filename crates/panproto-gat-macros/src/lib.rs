//! Proc-macro surface for declarative class and instance syntax
//! targeting panproto-gat.
//!
//! Provides three macros:
//!
//! - `class! { ThEq<A> { ... } }` expands to a `theory_<lowercase>()`
//!   function that returns a `panproto_gat::Theory` built from the
//!   listed signatures and axioms.
//! - `instance! { Name: Class<Ty, ...> in Target { op = target_op; ... } }`
//!   expands to an `instance_<lowercase>(class, target)` function that
//!   builds a validated `panproto_gat::TheoryMorphism`.
//! - `fn_with_constraints!` is a syntactic placeholder for the
//!   constrained-function sugar; it currently parses the form and
//!   emits a specific compile error directing users to the follow-up
//!   work.

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
    output: Ident,
}

struct ArgItem {
    name: Ident,
    ty: Ident,
}

impl Parse for ArgItem {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty: Ident = input.parse()?;
        Ok(Self { name, ty })
    }
}

/// AST for an equational axiom: `axiom name: lhs = rhs;`.
struct AxiomItem {
    name: Ident,
    lhs: TokenStream2,
    rhs: TokenStream2,
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
            let lhs = parse_until_eq(input)?;
            input.parse::<Token![=]>()?;
            let rhs = parse_until_semi(input)?;
            input.parse::<Token![;]>()?;
            return Ok(Self::Axiom(AxiomItem { name, lhs, rhs }));
        }

        // Parse signature: name(args) -> Output;
        let name: Ident = input.parse()?;
        let content;
        parenthesized!(content in input);
        let args = Punctuated::<ArgItem, Token![,]>::parse_terminated(&content)?;
        input.parse::<Token![->]>()?;
        let output: Ident = input.parse()?;
        input.parse::<Token![;]>()?;
        Ok(Self::Sig(SigItem { name, args, output }))
    }
}

fn parse_until_eq(input: ParseStream<'_>) -> syn::Result<TokenStream2> {
    let mut out = TokenStream2::new();
    while !input.is_empty() && !input.peek(Token![=]) {
        let tt: proc_macro2::TokenTree = input.parse()?;
        out.extend(std::iter::once(tt));
    }
    if input.is_empty() {
        return Err(input.error("expected `=` in axiom"));
    }
    Ok(out)
}

fn parse_until_semi(input: ParseStream<'_>) -> syn::Result<TokenStream2> {
    let mut out = TokenStream2::new();
    while !input.is_empty() && !input.peek(Token![;]) {
        let tt: proc_macro2::TokenTree = input.parse()?;
        out.extend(std::iter::once(tt));
    }
    if input.is_empty() {
        return Err(input.error("expected `;` at end of axiom"));
    }
    Ok(out)
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
#[proc_macro]
pub fn class(input: TokenStream) -> TokenStream {
    let ClassInput {
        name,
        params,
        items,
    } = parse_macro_input!(input as ClassInput);

    let name_str = name.to_string();
    let fn_name = format_ident!("theory_{}", name_str.to_lowercase());

    let sort_names: Vec<String> = params.iter().map(ToString::to_string).collect();

    let sort_inits = sort_names.iter().map(|n| {
        quote! { ::panproto_gat::Sort::simple(#n) }
    });

    let mut op_inits = Vec::new();
    let mut eq_inits = Vec::new();

    for item in items {
        match item {
            ClassBodyItem::Sig(SigItem {
                name: op_name,
                args,
                output,
            }) => {
                let op_name_str = op_name.to_string();
                let output_str = output.to_string();
                let arg_triples = args.iter().map(|ArgItem { name, ty }| {
                    let n = name.to_string();
                    let t = ty.to_string();
                    quote! {
                        (
                            ::std::sync::Arc::from(#n),
                            ::panproto_gat::SortExpr::Name(::std::sync::Arc::from(#t)),
                            ::panproto_gat::Implicit::No,
                        )
                    }
                });
                op_inits.push(quote! {
                    ::panproto_gat::Operation::with_implicit(
                        #op_name_str,
                        ::std::vec![ #( #arg_triples ),* ],
                        ::panproto_gat::SortExpr::Name(::std::sync::Arc::from(#output_str)),
                    )
                });
            }
            ClassBodyItem::Axiom(AxiomItem {
                name: ax_name,
                lhs,
                rhs,
            }) => {
                let ax_name_str = ax_name.to_string();
                let lhs_str = lhs.to_string();
                let rhs_str = rhs.to_string();
                let lhs_tokens = match term_tokens(&lhs_str) {
                    Ok(t) => t,
                    Err(e) => {
                        return syn::Error::new(
                            ax_name.span(),
                            format!("axiom lhs parse error: {e}"),
                        )
                        .to_compile_error()
                        .into();
                    }
                };
                let rhs_tokens = match term_tokens(&rhs_str) {
                    Ok(t) => t,
                    Err(e) => {
                        return syn::Error::new(
                            ax_name.span(),
                            format!("axiom rhs parse error: {e}"),
                        )
                        .to_compile_error()
                        .into();
                    }
                };
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

    let doc = format!("Construct the `{name_str}` theory produced by the `class!` macro.");
    let expanded = quote! {
        #[doc = #doc]
        pub fn #fn_name() -> ::panproto_gat::Theory {
            ::panproto_gat::Theory::new(
                #name_str,
                ::std::vec![ #( #sort_inits ),* ],
                ::std::vec![ #( #op_inits ),* ],
                ::std::vec![ #( #eq_inits ),* ],
            )
        }
    };

    expanded.into()
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
// fn_with_constraints! macro (parse-only stub)
// ═══════════════════════════════════════════════════════════════════

struct FnWithConstraintsInput {
    _raw: TokenStream2,
}

impl Parse for FnWithConstraintsInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        // Accept the form:
        //   name<A: Class, ...>(args) -> Out { body }
        // but do not compile it; we only want the parse tree to be
        // well-defined so a future expansion can use it.
        let _name: Ident = input.parse()?;
        input.parse::<Token![<]>()?;
        // Parse `A: Class` or `A: Class1 + Class2`-style entries until `>`.
        while !input.peek(Token![>]) {
            let _param: Ident = input.parse()?;
            input.parse::<Token![:]>()?;
            // Consume one or more idents separated by `+`.
            let _: Ident = input.parse()?;
            while input.peek(Token![+]) {
                input.parse::<Token![+]>()?;
                let _: Ident = input.parse()?;
            }
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }
        input.parse::<Token![>]>()?;
        let _args;
        parenthesized!(_args in input);
        // Consume the rest.
        let mut raw = TokenStream2::new();
        while !input.is_empty() {
            let tt: proc_macro2::TokenTree = input.parse()?;
            raw.extend(std::iter::once(tt));
        }
        Ok(Self { _raw: raw })
    }
}

/// Parse the constrained-function surface and emit a compile error
/// directing users that the expansion is queued as follow-up work.
#[proc_macro]
pub fn fn_with_constraints(input: TokenStream) -> TokenStream {
    let _ = parse_macro_input!(input as FnWithConstraintsInput);
    let err = syn::Error::new(
        proc_macro2::Span::call_site(),
        "constrained-function sugar is queued as follow-up",
    );
    err.to_compile_error().into()
}

// ═══════════════════════════════════════════════════════════════════
// Compile-time term parsing for axiom lhs/rhs
// ═══════════════════════════════════════════════════════════════════

/// Parse a term source string into a `quote!`-able construction of a
/// `panproto_gat::Term`.
fn term_tokens(s: &str) -> Result<TokenStream2, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty term".to_owned());
    }
    match s.find('(') {
        None => {
            // Bare variable.
            validate_ident(s)?;
            Ok(quote! {
                ::panproto_gat::Term::Var(::std::sync::Arc::from(#s))
            })
        }
        Some(paren) => {
            let op = s[..paren].trim();
            validate_ident(op)?;
            let inner = &s[paren + 1..];
            let close =
                find_matching_paren(inner).ok_or_else(|| format!("unclosed paren in {s:?}"))?;
            let args_str = &inner[..close];
            let trailing = inner[close + 1..].trim();
            if !trailing.is_empty() {
                return Err(format!("trailing input after `)` in {s:?}: {trailing:?}"));
            }
            let args = split_top_commas(args_str)
                .into_iter()
                .map(term_tokens)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(quote! {
                ::panproto_gat::Term::App {
                    op: ::std::sync::Arc::from(#op),
                    args: ::std::vec![ #( #args ),* ],
                }
            })
        }
    }
}

fn validate_ident(s: &str) -> Result<(), String> {
    let mut chars = s.chars();
    let first = chars.next().ok_or_else(|| "empty identifier".to_owned())?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(format!("identifier {s:?} must start with a letter or `_`"));
    }
    for c in chars {
        if !(c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!("identifier {s:?} contains invalid char {c:?}"));
        }
    }
    Ok(())
}

fn find_matching_paren(s: &str) -> Option<usize> {
    let mut depth = 1i32;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_top_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                let p = s[start..i].trim();
                if !p.is_empty() {
                    parts.push(p);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = s[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }
    parts
}

use panproto_gat_macros::derive_theory;

// `derive_theory!` requires a parenthesized trait list in the leading
// `#[derive(...)]` attribute. Without parentheses the macro must emit
// a diagnostic rather than panic.
derive_theory! {
    #[derive]
    theory ThEmpty {
        sort A;
    }
}

fn main() {}

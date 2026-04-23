use panproto_gat_macros::fn_with_constraints;

fn_with_constraints! {
    elem<A: ThEq>(x: A) -> Bool { eq(x, x) }
}

fn main() {}

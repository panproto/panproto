use panproto_gat_macros::class;

class! {
    ThEq<A> {
        eq(x: A, y: A) -> A;
        axiom bad: 1 + 2 = eq(x, x);
    }
}

fn main() {}

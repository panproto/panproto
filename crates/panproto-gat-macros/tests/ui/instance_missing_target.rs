use panproto_gat_macros::instance;

instance! {
    EqInt: ThEq<Int> {
        eq = int_eq;
    }
}

fn main() {}

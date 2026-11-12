#[cfg(test)]
mod tests {
    #[test]
    fn print_trees() {
        for src in [
            "INSERT INTO public.t VALUES (1, -2, 1.5e3, 'a''b', E'x\\ny', $$d$$, TRUE, NULL, '{1,2}', -0.0, +3);",
            "Insert into language\n (language_id,name)\nValues\n('1','English')\n;",
            "INSERT INTO t (a, b) VALUES (1, DEFAULT), (2, 'x'::text), (3, DATE '2020-01-01'), (4, 'a'\n'b'), (U&'d\\0061t', 0x1F);",
            "INSERT INTO t OVERRIDING SYSTEM VALUE VALUES (1);",
            "INSERT INTO t SELECT 1;",
            "INSERT INTO t DEFAULT VALUES;",
            "SET standard_conforming_strings = off; SET client_encoding = 'UTF8'; ALTER TABLE language DISABLE TRIGGER ALL; SELECT pg_catalog.setval('a_seq', 200, true);",
            "INSERT INTO t VALUES (- -1, (1), ARRAY[1,2], 1_000);",
        ] {
            let tree = super::super::parse(src).unwrap();
            println!("{src}\n{}\n", tree.root_node().to_sexp());
        }
    }
}

# Table lens fixtures

Each case manifest names the source and target protocol contexts used by its
`get` and `put` replay. The phase-2 table-lens test executes every manifest and
checks that the set covers every table put-domain class and every error variant
that a table lens can raise. The later corpus lane adds the frozen wire payloads
consumed by language bindings without changing these cases.

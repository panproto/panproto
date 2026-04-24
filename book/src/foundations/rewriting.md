# Rewriting, confluence, and termination

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

Equations in a GAT are undirected. An equation $s = t$ says nothing about which side is the normal form; it says only that the theory's models identify the two terms. The engine needs more. When a REPL user asks to normalize a term, or when the typechecker wants to decide whether two sides of a proposed equation are equal up to the existing rules, a symmetric relation is the wrong thing to reason with. The engine needs a direction, and it needs guarantees that the directed relation is well-behaved. This chapter is about the two guarantees that matter, and the two reports in `panproto-gat::rewriting` that supply them.

A `DirectedEquation` is an equation with an oriented LHS and RHS: rewriting proceeds from LHS to RHS, and the inverse direction is optional. The engine uses directed equations for three distinct purposes. Coercion round-trips have been the original motivating case: a coercion and its inverse form a directed pair, and the engine rewrites away the compose-with-inverse on normalization. The REPL's `:normalize` command is the second case. The third, and the one the 0.37 release adds, is the new `typecheck_equation_modulo_rewrites` path: a proposed equation is accepted if its two sides share a normal form under the theory's directed system, not only if they are already alpha-equivalent. Each of the three uses depends on the directed system behaving well; the `rewriting` module is where well-behaved is made precise.

## Termination

A rewrite system *terminates* if no term admits an infinite reduction sequence. A non-terminating system is one that can rewrite forever, and the engine calling `normalize` on a term under a non-terminating system will loop. The textbook check is the **lexicographic path order** (LPO), the simplification-order family introduced by Dershowitz in the early 1980s: pick a total order on the operation symbols (the *precedence*), then extend it to a term order by comparing heads first and, on tied heads, comparing argument lists lexicographically. A rewrite rule $l \to r$ is **LPO-compatible** if $l >_{\mathrm{lpo}} r$ for every ground substitution. A system whose rules are all LPO-compatible for a single precedence is guaranteed to terminate.

[`check_termination_via_lpo`](https://docs.rs/panproto-gat/latest/panproto_gat/rewriting/fn.check_termination_via_lpo.html) takes a list of directed equations and an `OpPrecedence` and returns a `TerminationReport`. The report lists any rule whose LPO comparison fails, with a `RuleViolation` identifying the offending rule and the specific LHS-versus-RHS comparison that did not go the right way. The raw comparator `lpo_greater` is exposed for callers who want to order terms directly.

The precedence is the user's design choice: a rule $\mathrm{compose}(\mathrm{id}, f) \to f$ needs $\mathrm{compose} > \mathrm{id}$ in the precedence, which matches the intuition that composition is the heavier symbol and identity is lighter. A precedence that puts the two in the other order will fail the LPO check, and the user is expected to notice and fix their rule direction. The engine does not invent the precedence; it checks the one the user supplies.

## Confluence

A rewrite system is *confluent* if every term that can reduce by two paths can eventually rejoin at a common reduct. A non-confluent system is one where the normal form depends on the order in which rules fire, which is a semantic bug: asking whether two terms are equal under the theory has multiple answers depending on how the engine chose to reduce them.

For directed equational systems without unrestricted left-hand sides, **local confluence** is decidable by the Knuth-Bendix critical-pair construction. Every pair of rules is examined at every overlap of their left-hand sides; the two reducts of the overlapped redex form a *critical pair*; and the system is locally confluent iff every critical pair joins, meaning that the two reducts reach a common normal form under the rest of the system. Newman's lemma then promotes local confluence to full confluence in the presence of termination; together with the LPO check above, local confluence is what the engine needs.

[`check_local_confluence`](https://docs.rs/panproto-gat/latest/panproto_gat/rewriting/fn.check_local_confluence.html) computes the critical pairs of a directed system and returns a `ConfluenceReport` listing every critical pair that does not join. Each `CriticalPair` carries the two rules that overlapped, the unified LHS they share, the two reducts, and a flag indicating whether the pair joined. A theory author reading a non-empty critical-pair list has concrete evidence of which pair of rules competes: the remedy is to orient one of them the other way, add an equation that forces the two reducts equal, or rewrite one rule's LHS so the overlap disappears.

## Why both together

A system that is confluent but not terminating can still reduce forever; a system that terminates but is not confluent produces different normal forms depending on reduction order. The engine needs both. The two reports are independent, and the typical workflow is to run `check_termination_via_lpo` first (a termination failure often masks a confluence problem by making the critical-pair completion diverge), then `check_local_confluence`, then iterate until both reports are clean. Once they are, `normalize` is a well-defined operation, `typecheck_equation_modulo_rewrites` has a terminating algorithm, and the REPL's `:normalize` command produces the unique answer the theory's equations demand.

## Further reading

The standard book-length treatment of term rewriting, confluence, and termination orderings is Baader and Nipkow's *Term Rewriting and All That* (1998), which covers every construction used in this chapter at the level of a working proof. The path-ordering family that LPO belongs to was introduced by Dershowitz in the early 1980s; the critical-pair / completion procedure is Knuth and Bendix's *Simple Word Problems in Universal Algebras* (1970). Bachmair and Ganzinger's work in the late 1980s gives the modern ordered-completion refinement that underlies most production implementations.

## Closing

With a well-behaved directed system in hand, the engine can normalize terms and check equations modulo rewriting. The next chapter opens Part II by translating this chapter's mathematics into the language of the schemas working programmers use every day.

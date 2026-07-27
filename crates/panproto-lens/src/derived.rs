//! Derived view components: the part of a view that `get` recomputes.
//!
//! A field transform that *materializes* a value (`ComputeField`,
//! `AddField`, a non-invertible `ApplyExpr`) makes the view component it
//! writes a function of the other components rather than an independent
//! coordinate. `get` recomputes it on every pass, so a caller-supplied view
//! carrying a stale copy of it is not in the image of `get`.
//!
//! This matters for `PutGet`. The law `get(put(s, v)) = v` quantifies over
//! views, but a lens with a derived component does not have a free view
//! space: the derived coordinate is pinned by the independent ones. Editing
//! an independent coordinate without re-deriving leaves a view that no
//! source maps to, and `get(put(s, v))` rightly disagrees with it on the
//! derived coordinate alone. Checking the law against such a view reports a
//! violation that says nothing about the lens.
//!
//! So `PutGet` is checked **modulo derived components**: `get(put(s, v))`
//! must agree with `v` on every independent coordinate, and the derived
//! coordinates are whatever the forward pass deterministically recomputes.
//! This is the property the `ComputeField` documentation already states for
//! `Opaque` computations, extended to every transform that materializes a
//! value. `GetPut` is unaffected and stays strict: it quantifies over
//! sources, and its view argument is `get(s)`, which is consistent by
//! construction.
//!
//! An inverse alone does not make a coordinate independent. What matters is
//! whether the transform *replaces* the coordinate it read or *adds* one
//! beside it. `up = upper(a)` with `a` still in the view holds the same
//! information twice, so `get` recomputes `up` from `a` and the inverse
//! never gets a say; drop `a` in the same batch and `up` becomes the only
//! carrier, `put` inverts it back, and the round trip is exact. The same
//! distinction decides an `ApplyExpr`: over an `extra_fields` entry it reads
//! and writes one slot and so swaps the coordinate, while over a child
//! scalar it reads the child and writes a shadowing entry, leaving both in
//! the view.

use std::collections::{HashMap, HashSet};

use panproto_gat::{CoercionClass, Name};
use panproto_inst::{CompiledMigration, FieldTransform};

/// The derived coordinates of the fiber over one vertex.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DerivedFiber {
    /// Derived `extra_fields` locations as `(path, key)`, where `path` is
    /// the `PathTransform` nesting that reaches the map holding `key`.
    locations: HashSet<(Vec<String>, String)>,
    /// Whether the node's own leaf value (`__value__`) is derived.
    value: bool,
}

impl DerivedFiber {
    /// Whether this fiber has no derived coordinates.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.locations.is_empty() && !self.value
    }

    /// Whether the node's leaf value is derived.
    #[must_use]
    pub const fn value_is_derived(&self) -> bool {
        self.value
    }

    /// The keys derived at nesting `path`.
    fn keys_at<'a>(&'a self, path: &'a [String]) -> impl Iterator<Item = &'a str> {
        self.locations
            .iter()
            .filter(move |(p, _)| p.as_slice() == path)
            .map(|(_, k)| k.as_str())
    }

    /// Whether any derived location nests strictly below `path + [key]`.
    fn has_nested_below(&self, path: &[String], key: &str) -> bool {
        self.locations.iter().any(|(p, _)| {
            p.len() > path.len() && p.starts_with(path) && p[path.len()].as_str() == key
        })
    }
}

/// Derived coordinates per anchor vertex.
pub type DerivedMap = HashMap<Name, DerivedFiber>;

/// Collect the coordinates that `get` recomputes, keyed by anchor vertex.
///
/// A transform contributes a derived coordinate when it materializes a
/// value that `put` cannot send back to an independent source coordinate:
///
/// * `ComputeField`: derived unless it *replaces* its source, in the sense
///   of [`replaces_its_source`].
/// * `ApplyExpr`: derived unless it is invertible *and* its key is an
///   `extra_fields` entry rather than a child edge, in which case it reads
///   and writes the same slot and so swaps the coordinate rather than
///   adding one.
/// * `AddField`: always derived, since the source has no such coordinate, so
///   `put` drops the key and the next `get` re-adds the constant.
/// * `PathTransform`: recursed into under its path prefix.
/// * `Case`: the union over all branches, since which branch fires depends
///   on the value and any of them may materialize its target.
///
/// `RenameField`, `DropField`, `KeepFields`, and `MapReferences` materialize
/// nothing and so contribute no derived coordinate.
#[must_use]
pub fn collect_derived_fields(compiled: &CompiledMigration) -> DerivedMap {
    let mut map = DerivedMap::new();
    for (anchor, transforms) in &compiled.field_transforms {
        let children = child_field_names(compiled, anchor);
        let mut fiber = DerivedFiber::default();
        collect_into(&mut fiber, transforms, &[], &children);
        if !fiber.is_empty() {
            map.insert(anchor.clone(), fiber);
        }
    }
    map
}

/// The environment keys the surviving child edges of `anchor` contribute,
/// matching `collect_scalar_child_values`.
///
/// A transform writing one of these names does not overwrite the child; it
/// shadows it with an `extra_fields` entry that serialization prefers. The
/// child still carries the original value, so the view holds the same
/// information twice.
fn child_field_names(compiled: &CompiledMigration, anchor: &Name) -> HashSet<String> {
    compiled
        .surviving_edges
        .iter()
        .filter(|edge| &edge.src == anchor)
        .map(|edge| edge.name.as_deref().unwrap_or(&edge.tgt).to_string())
        .collect()
}

/// Whether a transform's declared inversion genuinely round-trips.
const fn is_invertible(coercion_class: CoercionClass, has_inverse: bool) -> bool {
    matches!(coercion_class, CoercionClass::Iso) && has_inverse
}

/// The single field a `ComputeField`'s forward expression reads, when there
/// is exactly one.
///
/// An inverse expression yields one value, so it can only restore one source
/// coordinate. A forward expression over several fields has no single
/// coordinate to invert to, and one over none is a constant; neither is a
/// bijection, whatever `coercion_class` claims.
#[must_use]
pub(crate) fn sole_source_var(expr: &panproto_expr::Expr) -> Option<String> {
    let free = panproto_expr::free_vars(expr);
    if free.len() == 1 {
        free.into_iter().next().map(|v| v.to_string())
    } else {
        None
    }
}

/// Whether a `ComputeField` replaces the coordinate it reads rather than
/// adding one beside it.
///
/// Adding `up = upper(a)` while `a` stays in the view leaves the two
/// redundant: `up` is a function of `a`, so editing `a` alone yields a view
/// no source maps to, and `get` recomputes `up` from `a` regardless of what
/// the inverse would have said. An inverse does not change that; the
/// coordinate is still derived.
///
/// Dropping `a` in the same batch is what makes the transform a genuine
/// change of coordinates: `up` becomes the only carrier of that information,
/// `put` inverts it back to `a`, and the round trip is exact.
///
/// So this requires the transform to be invertible, to read exactly one
/// field, and for a later transform on the same anchor to remove that field.
/// A source that is a *child scalar* rather than an `extra_fields` entry is
/// reported as not replaced: `DropField` and `KeepFields` filter
/// `extra_fields` only, so nothing in the transform list can remove it, and
/// treating the target as derived is the conservative reading (it weakens
/// the comparison rather than reporting a violation that is not there).
fn replaces_its_source(
    expr: &panproto_expr::Expr,
    inverse: Option<&panproto_expr::Expr>,
    coercion_class: CoercionClass,
    removed: &HashSet<String>,
) -> bool {
    is_invertible(coercion_class, inverse.is_some())
        && sole_source_var(expr).is_some_and(|v| removed.contains(&v))
}

/// The top-level `extra_fields` keys whose information this transform list
/// takes out of the view entirely.
///
/// `DropField` and `KeepFields` are the two that do so. A `RenameField` is
/// deliberately excluded: it moves the value to another key, where it is
/// still in the view, so a computation over the old name remains redundant
/// with it rather than replacing it.
fn removed_keys(transforms: &[FieldTransform]) -> HashSet<String> {
    let mut removed = HashSet::new();
    let mut keep: Option<HashSet<String>> = None;
    for transform in transforms {
        match transform {
            FieldTransform::DropField { key } => {
                removed.insert(key.clone());
            }
            FieldTransform::KeepFields { keys } => {
                let next: HashSet<String> = keys.iter().cloned().collect();
                keep = Some(match keep {
                    None => next,
                    Some(prev) => prev.intersection(&next).cloned().collect(),
                });
            }
            _ => {}
        }
    }
    // A `KeepFields` removes every top-level key outside its retain set.
    // The only names that matter here are the ones a `ComputeField` reads,
    // so test those against the set rather than enumerating a complement
    // that has no bound.
    if let Some(keep) = keep {
        for transform in transforms {
            if let FieldTransform::ComputeField { expr, .. } = transform
                && let Some(v) = sole_source_var(expr)
                && !keep.contains(&v)
            {
                removed.insert(v);
            }
        }
    }
    removed
}

fn collect_into(
    fiber: &mut DerivedFiber,
    transforms: &[FieldTransform],
    path: &[String],
    children: &HashSet<String>,
) {
    let removed = removed_keys(transforms);
    for transform in transforms {
        match transform {
            FieldTransform::ComputeField {
                target_key,
                expr,
                inverse,
                coercion_class,
            } => {
                if !replaces_its_source(expr, inverse.as_ref(), *coercion_class, &removed)
                    || children.contains(target_key)
                {
                    fiber.locations.insert((path.to_vec(), target_key.clone()));
                }
            }
            FieldTransform::ApplyExpr {
                key,
                inverse,
                coercion_class,
                ..
            } => {
                // An invertible `ApplyExpr` over an `extra_fields` entry
                // reads and writes the same slot, so it swaps the
                // coordinate for another and stays independent. Over a
                // *child scalar* it reads the child but writes an
                // `extra_fields` entry, leaving the child's value still in
                // the view beside a function of it; that is a derived
                // coordinate however the transform is classified.
                let swaps_in_place =
                    is_invertible(*coercion_class, inverse.is_some()) && !children.contains(key);
                if swaps_in_place {
                    continue;
                }
                if key == "__value__" {
                    fiber.value = true;
                } else {
                    fiber.locations.insert((path.to_vec(), key.clone()));
                }
            }
            FieldTransform::AddField { key, .. } => {
                fiber.locations.insert((path.to_vec(), key.clone()));
            }
            FieldTransform::PathTransform {
                path: inner_path,
                inner,
            } => {
                let mut nested = path.to_vec();
                nested.extend(inner_path.iter().cloned());
                collect_into(fiber, std::slice::from_ref(inner), &nested, children);
            }
            FieldTransform::Case { branches } => {
                for branch in branches {
                    collect_into(fiber, &branch.transforms, path, children);
                }
            }
            FieldTransform::RenameField { .. }
            | FieldTransform::DropField { .. }
            | FieldTransform::KeepFields { .. }
            | FieldTransform::MapReferences { .. } => {}
        }
    }
}

/// Compare two `extra_fields` maps, ignoring the derived locations.
///
/// Returns `None` when they agree on every independent coordinate, or
/// `Some(description)` naming the first disagreement found.
pub(crate) fn extra_fields_equiv_modulo(
    a: &HashMap<String, panproto_inst::value::Value>,
    b: &HashMap<String, panproto_inst::value::Value>,
    fiber: &DerivedFiber,
    path: &[String],
) -> Option<String> {
    use panproto_inst::value::Value;

    let ignored: HashSet<&str> = fiber.keys_at(path).collect();
    let key_label = |k: &str| {
        if path.is_empty() {
            k.to_string()
        } else {
            format!("{}.{k}", path.join("."))
        }
    };

    for (key, va) in a {
        if ignored.contains(key.as_str()) {
            continue;
        }
        let Some(vb) = b.get(key) else {
            return Some(format!(
                "field `{}` present in view, absent after re-get",
                key_label(key)
            ));
        };
        // Recurse into a nested map only when a derived location sits
        // below it; otherwise the whole subtree compares structurally.
        if fiber.has_nested_below(path, key) {
            if let (Value::Unknown(ma), Value::Unknown(mb)) = (va, vb) {
                let mut nested = path.to_vec();
                nested.push(key.clone());
                if let Some(detail) = extra_fields_equiv_modulo(ma, mb, fiber, &nested) {
                    return Some(detail);
                }
                continue;
            }
        }
        if !crate::asymmetric::value_equiv(va, vb) {
            return Some(format!(
                "field `{}` differs: view has {va:?}, re-get has {vb:?}",
                key_label(key)
            ));
        }
    }

    for key in b.keys() {
        if ignored.contains(key.as_str()) || a.contains_key(key) {
            continue;
        }
        return Some(format!(
            "field `{}` absent in view, present after re-get",
            key_label(key)
        ));
    }

    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn compute(target: &str, class: CoercionClass, inverse: bool) -> FieldTransform {
        FieldTransform::ComputeField {
            target_key: target.into(),
            expr: panproto_expr::Expr::Var(Arc::from("a")),
            inverse: inverse.then(|| panproto_expr::Expr::Var(Arc::from(target))),
            coercion_class: class,
        }
    }

    fn migration(anchor: &str, transforms: Vec<FieldTransform>) -> CompiledMigration {
        let mut field_transforms = HashMap::new();
        field_transforms.insert(Name::from(anchor), transforms);
        CompiledMigration {
            surviving_verts: HashSet::new(),
            surviving_edges: HashSet::new(),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms,
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        }
    }

    #[test]
    fn projection_compute_field_is_derived() {
        let m = migration(
            "root",
            vec![compute("grp", CoercionClass::Projection, false)],
        );
        let derived = collect_derived_fields(&m);
        let fiber = derived.get(&Name::from("root")).expect("anchor present");
        assert!(
            fiber.locations.contains(&(vec![], "grp".to_string())),
            "a Projection ComputeField materializes a derived coordinate"
        );
    }

    #[test]
    fn invertible_compute_field_is_not_derived() {
        let m = migration("root", vec![compute("grp", CoercionClass::Iso, true)]);
        assert!(
            collect_derived_fields(&m).is_empty(),
            "an Iso ComputeField with an inverse stays an independent coordinate"
        );
    }

    #[test]
    fn iso_without_inverse_is_derived() {
        let m = migration("root", vec![compute("grp", CoercionClass::Iso, false)]);
        let derived = collect_derived_fields(&m);
        assert!(
            derived.contains_key(&Name::from("root")),
            "an Iso claim without an inverse expression cannot be inverted by put"
        );
    }

    #[test]
    fn path_transform_records_nested_location() {
        let m = migration(
            "root",
            vec![FieldTransform::PathTransform {
                path: vec!["attrs".into()],
                inner: Box::new(compute("slug", CoercionClass::Projection, false)),
            }],
        );
        let derived = collect_derived_fields(&m);
        let fiber = derived.get(&Name::from("root")).expect("anchor present");
        assert!(
            fiber
                .locations
                .contains(&(vec!["attrs".to_string()], "slug".to_string())),
            "PathTransform nesting is recorded on the derived location: {fiber:?}"
        );
    }

    #[test]
    fn case_branches_union_their_derived_targets() {
        let m = migration(
            "root",
            vec![FieldTransform::Case {
                branches: vec![
                    panproto_inst::wtype::CaseBranch {
                        predicate: panproto_expr::Expr::Lit(panproto_expr::Literal::Bool(true)),
                        transforms: vec![compute("x", CoercionClass::Projection, false)],
                    },
                    panproto_inst::wtype::CaseBranch {
                        predicate: panproto_expr::Expr::Lit(panproto_expr::Literal::Bool(false)),
                        transforms: vec![compute("y", CoercionClass::Projection, false)],
                    },
                ],
            }],
        );
        let derived = collect_derived_fields(&m);
        let fiber = derived.get(&Name::from("root")).expect("anchor present");
        assert!(
            fiber.locations.contains(&(vec![], "x".to_string()))
                && fiber.locations.contains(&(vec![], "y".to_string())),
            "every branch target is derived, since which branch fires is value-dependent"
        );
    }

    #[test]
    fn rename_and_drop_derive_nothing() {
        let m = migration(
            "root",
            vec![
                FieldTransform::RenameField {
                    old_key: "a".into(),
                    new_key: "b".into(),
                },
                FieldTransform::DropField { key: "c".into() },
            ],
        );
        assert!(
            collect_derived_fields(&m).is_empty(),
            "renames and drops materialize no value"
        );
    }

    #[test]
    fn modulo_comparison_ignores_only_the_derived_key() {
        use panproto_inst::value::Value;
        let mut fiber = DerivedFiber::default();
        fiber.locations.insert((vec![], "grp".to_string()));

        let a = HashMap::from([
            ("grp".to_string(), Value::Str("stale".into())),
            ("keep".to_string(), Value::Str("x".into())),
        ]);
        let mut b = HashMap::from([
            ("grp".to_string(), Value::Str("fresh".into())),
            ("keep".to_string(), Value::Str("x".into())),
        ]);
        assert!(
            extra_fields_equiv_modulo(&a, &b, &fiber, &[]).is_none(),
            "the derived key is excluded from the comparison"
        );

        b.insert("keep".to_string(), Value::Str("drifted".into()));
        let detail = extra_fields_equiv_modulo(&a, &b, &fiber, &[]);
        assert!(
            detail.is_some_and(|d| d.contains("keep")),
            "an independent key still has to agree"
        );
    }
}

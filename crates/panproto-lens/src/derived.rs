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
//! A transform classified `Iso` *with* an inverse is not derived: `put`
//! inverts it, so the coordinate is independent and stays under the strict
//! comparison.

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
/// * `ComputeField` / `ApplyExpr`: derived unless classified [`CoercionClass::Iso`]
///   *and* carrying an inverse, which is the case `put` genuinely inverts.
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
        let mut fiber = DerivedFiber::default();
        collect_into(&mut fiber, transforms, &[]);
        if !fiber.is_empty() {
            map.insert(anchor.clone(), fiber);
        }
    }
    map
}

/// Whether a transform's declared inversion genuinely round-trips.
const fn is_invertible(coercion_class: CoercionClass, has_inverse: bool) -> bool {
    matches!(coercion_class, CoercionClass::Iso) && has_inverse
}

fn collect_into(fiber: &mut DerivedFiber, transforms: &[FieldTransform], path: &[String]) {
    for transform in transforms {
        match transform {
            FieldTransform::ComputeField {
                target_key,
                inverse,
                coercion_class,
                ..
            } => {
                if !is_invertible(*coercion_class, inverse.is_some()) {
                    fiber.locations.insert((path.to_vec(), target_key.clone()));
                }
            }
            FieldTransform::ApplyExpr {
                key,
                inverse,
                coercion_class,
                ..
            } => {
                if is_invertible(*coercion_class, inverse.is_some()) {
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
                collect_into(fiber, std::slice::from_ref(inner), &nested);
            }
            FieldTransform::Case { branches } => {
                for branch in branches {
                    collect_into(fiber, &branch.transforms, path);
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

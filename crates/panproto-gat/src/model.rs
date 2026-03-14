use std::fmt;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::error::GatError;
use crate::morphism::TheoryMorphism;

/// A value in a model interpretation.
///
/// `ModelValue` represents the elements that sorts are interpreted as,
/// and the values that operations produce and consume.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum ModelValue {
    /// A string value.
    Str(String),
    /// A 64-bit integer value.
    Int(i64),
    /// A boolean value.
    Bool(bool),
    /// A list of values.
    List(Vec<Self>),
    /// A map of key-value pairs.
    Map(FxHashMap<String, Self>),
    /// A null / absent value.
    Null,
}

/// An operation interpreter: a function from argument values to a result value.
///
/// Wrapped in `Arc` so that `Model` can be cloned and sent across threads.
type OpInterp = Arc<dyn Fn(&[ModelValue]) -> Result<ModelValue, GatError> + Send + Sync>;

/// A model (interpretation) of a theory in Set.
///
/// Maps each sort to a carrier set of values and each operation to a
/// function on those values. Models are the semantic counterpart of
/// theories: a theory describes structure abstractly, while a model
/// provides a concrete instantiation.
///
/// `Model` does not derive `Serialize`/`Deserialize` because `op_interp`
/// contains function pointers (`Arc<dyn Fn(...)>`) which cannot be serialized.
pub struct Model {
    /// The name of the theory this model interprets.
    pub theory: String,
    /// Sort interpretations: each sort name maps to its carrier set.
    pub sort_interp: FxHashMap<String, Vec<ModelValue>>,
    /// Operation interpretations: each operation name maps to a function.
    pub op_interp: FxHashMap<String, OpInterp>,
}

impl fmt::Debug for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Model")
            .field("theory", &self.theory)
            .field("sort_interp", &self.sort_interp)
            .field("op_interp_keys", &self.op_interp.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Model {
    /// Create a new model for a given theory.
    #[must_use]
    pub fn new(theory: impl Into<String>) -> Self {
        Self {
            theory: theory.into(),
            sort_interp: FxHashMap::default(),
            op_interp: FxHashMap::default(),
        }
    }

    /// Add a sort interpretation (carrier set).
    pub fn add_sort(&mut self, name: impl Into<String>, values: Vec<ModelValue>) {
        self.sort_interp.insert(name.into(), values);
    }

    /// Add an operation interpretation.
    pub fn add_op<F>(&mut self, name: impl Into<String>, f: F)
    where
        F: Fn(&[ModelValue]) -> Result<ModelValue, GatError> + Send + Sync + 'static,
    {
        self.op_interp.insert(name.into(), Arc::new(f));
    }

    /// Evaluate an operation by name on the given arguments.
    ///
    /// # Errors
    ///
    /// Returns [`GatError::OpNotFound`] if the operation is not in this model,
    /// or [`GatError::ModelError`] if the operation function itself fails.
    pub fn eval(&self, op_name: &str, args: &[ModelValue]) -> Result<ModelValue, GatError> {
        let f = self
            .op_interp
            .get(op_name)
            .ok_or_else(|| GatError::OpNotFound(op_name.to_owned()))?;
        f(args)
    }
}

/// Migrate a model along a theory morphism.
///
/// Given a morphism from theory A to theory B and a model of B, produce
/// a model of A by reindexing sort and operation interpretations via the
/// morphism's mappings.
///
/// Sort interpretations are renamed: if the morphism maps sort `S` to `T`,
/// then the new model's interpretation for `S` is taken from the original
/// model's interpretation for `T`.
///
/// Operation interpretations are renamed analogously.
///
/// # Errors
///
/// Returns [`GatError::ModelError`] if a mapped sort or operation is missing
/// from the source model.
pub fn migrate_model(morphism: &TheoryMorphism, model: &Model) -> Result<Model, GatError> {
    let mut new_model = Model::new(&model.theory);

    // Reindex sort interpretations.
    for (domain_sort, codomain_sort) in &morphism.sort_map {
        let values = model
            .sort_interp
            .get(codomain_sort.as_ref())
            .ok_or_else(|| {
                GatError::ModelError(format!(
                    "sort interpretation for '{codomain_sort}' not found in model"
                ))
            })?;
        new_model
            .sort_interp
            .insert(domain_sort.to_string(), values.clone());
    }

    // Reindex operation interpretations.
    for (domain_op, codomain_op) in &morphism.op_map {
        let interp = model.op_interp.get(codomain_op.as_ref()).ok_or_else(|| {
            GatError::ModelError(format!(
                "operation interpretation for '{codomain_op}' not found in model"
            ))
        })?;
        new_model
            .op_interp
            .insert(domain_op.to_string(), Arc::clone(interp));
    }

    Ok(new_model)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn int_val(v: i64) -> ModelValue {
        ModelValue::Int(v)
    }

    #[test]
    fn integer_monoid_model() {
        let mut model = Model::new("Monoid");

        // Carrier = {0, 1, 2, ..., 9}
        let carrier: Vec<ModelValue> = (0..10).map(int_val).collect();
        model.add_sort("Carrier", carrier);

        // mul = addition
        model.add_op("mul", |args: &[ModelValue]| match (&args[0], &args[1]) {
            (ModelValue::Int(a), ModelValue::Int(b)) => Ok(ModelValue::Int(a + b)),
            _ => Err(GatError::ModelError("expected Int arguments".to_owned())),
        });

        // unit = 0
        model.add_op("unit", |_args: &[ModelValue]| Ok(ModelValue::Int(0)));

        // Verify mul(3, 4) = 7.
        let result = model.eval("mul", &[int_val(3), int_val(4)]).unwrap();
        assert_eq!(result, int_val(7));

        // Verify unit() = 0.
        let result = model.eval("unit", &[]).unwrap();
        assert_eq!(result, int_val(0));

        // Verify left identity: mul(unit(), x) = x.
        let zero = model.eval("unit", &[]).unwrap();
        let result = model.eval("mul", &[zero, int_val(5)]).unwrap();
        assert_eq!(result, int_val(5));

        // Verify right identity: mul(x, unit()) = x.
        let zero = model.eval("unit", &[]).unwrap();
        let result = model.eval("mul", &[int_val(5), zero]).unwrap();
        assert_eq!(result, int_val(5));

        // Verify associativity: mul(a, mul(b, c)) = mul(mul(a, b), c).
        let bc = model.eval("mul", &[int_val(2), int_val(3)]).unwrap();
        let lhs = model.eval("mul", &[int_val(1), bc]).unwrap();
        let ab = model.eval("mul", &[int_val(1), int_val(2)]).unwrap();
        let rhs = model.eval("mul", &[ab, int_val(3)]).unwrap();
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn migrate_model_renames_sorts_and_ops() {
        let mut model = Model::new("M2");
        model.add_sort("Carrier", vec![int_val(0), int_val(1)]);
        model.add_op("times", |args: &[ModelValue]| match (&args[0], &args[1]) {
            (ModelValue::Int(a), ModelValue::Int(b)) => Ok(ModelValue::Int(a * b)),
            _ => Err(GatError::ModelError("expected Int".to_owned())),
        });
        model.add_op("one", |_: &[ModelValue]| Ok(ModelValue::Int(1)));

        // Morphism: M1 -> M2, mapping mul->times, unit->one.
        let sort_map =
            std::collections::HashMap::from([(Arc::from("Carrier"), Arc::from("Carrier"))]);
        let op_map = std::collections::HashMap::from([
            (Arc::from("mul"), Arc::from("times")),
            (Arc::from("unit"), Arc::from("one")),
        ]);

        let morphism = TheoryMorphism::new("rename", "M1", "M2", sort_map, op_map);
        let migrated = migrate_model(&morphism, &model).unwrap();

        // Migrated model should have "mul" and "unit" as keys.
        assert!(migrated.sort_interp.contains_key("Carrier"));
        assert!(migrated.op_interp.contains_key("mul"));
        assert!(migrated.op_interp.contains_key("unit"));

        // And the operations should still work.
        let result = migrated.eval("mul", &[int_val(3), int_val(4)]).unwrap();
        assert_eq!(result, int_val(12));

        let result = migrated.eval("unit", &[]).unwrap();
        assert_eq!(result, int_val(1));
    }

    #[test]
    fn migrate_model_missing_sort_fails() {
        let model = Model::new("Empty");

        let sort_map = std::collections::HashMap::from([(Arc::from("S"), Arc::from("Missing"))]);

        let morphism = TheoryMorphism::new(
            "bad",
            "X",
            "Empty",
            sort_map,
            std::collections::HashMap::new(),
        );
        let result = migrate_model(&morphism, &model);
        assert!(matches!(result, Err(GatError::ModelError(_))));
    }

    #[test]
    fn eval_missing_op_fails() {
        let model = Model::new("Empty");
        let result = model.eval("nonexistent", &[]);
        assert!(matches!(result, Err(GatError::OpNotFound(_))));
    }

    #[test]
    fn model_value_serialization_roundtrip() {
        let values = vec![
            ModelValue::Str("hello".to_owned()),
            ModelValue::Int(42),
            ModelValue::Bool(true),
            ModelValue::List(vec![ModelValue::Int(1), ModelValue::Int(2)]),
            ModelValue::Map(FxHashMap::from_iter([(
                "key".to_owned(),
                ModelValue::Str("val".to_owned()),
            )])),
            ModelValue::Null,
        ];

        for val in &values {
            let json = serde_json::to_string(val).unwrap();
            let roundtripped: ModelValue = serde_json::from_str(&json).unwrap();
            assert_eq!(val, &roundtripped);
        }
    }

    #[test]
    fn model_value_nested_roundtrip() {
        let nested = ModelValue::Map(FxHashMap::from_iter([(
            "list".to_owned(),
            ModelValue::List(vec![
                ModelValue::Int(1),
                ModelValue::Map(FxHashMap::from_iter([(
                    "inner".to_owned(),
                    ModelValue::Bool(false),
                )])),
            ]),
        )]));

        let json = serde_json::to_string(&nested).unwrap();
        let roundtripped: ModelValue = serde_json::from_str(&json).unwrap();
        assert_eq!(nested, roundtripped);
    }

    #[test]
    fn model_debug_format() {
        let model = Model::new("Test");
        let debug_str = format!("{model:?}");
        assert!(debug_str.contains("Test"));
    }
}

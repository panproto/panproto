//! Reference semantics partitioned by correctness page.

pub mod ansible;
pub mod bson;
pub mod cloudformation;
pub mod edge_lists;
pub mod edits;
pub mod instance_version_control;
pub mod json_schema;
pub mod kan_extensions;
pub mod keys_and_registration;
pub mod kubernetes_crds;
pub mod lenses;
mod lexical_formats;
pub mod migration_search;
pub mod postgresql_data;
pub mod postgresql_ddl;
pub mod rdf_formats;
pub mod representation;
pub mod resolution;
pub mod restrict_and_contraction;
pub mod schema_merge;
pub mod segments;
pub mod text_formats;
pub mod theories;
pub mod theory_dsl;
pub mod validation;
pub mod value_io;
pub mod values;

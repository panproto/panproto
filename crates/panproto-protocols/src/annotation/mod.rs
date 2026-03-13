//! Linguistic annotation format protocol definitions.
//!
//! This module covers annotation standards used in computational linguistics
//! and NLP, as used by the Layers annotation platform and similar tools.

/// AMR (Abstract Meaning Representation) protocol.
pub mod amr;
/// bead annotation protocol.
pub mod bead;
/// brat standoff annotation protocol.
pub mod brat;
/// Concrete (JHU HLTCOE) protocol.
pub mod concrete;
/// CoNLL-U (Universal Dependencies) protocol.
pub mod conllu;
/// Decomp/UDS (Decompositional Semantics / Universal Decompositional Semantics) protocol.
pub mod decomp;
/// ELAN/Praat time-aligned annotation protocol.
pub mod elan;
/// FoLiA (Format for Linguistic Annotation) protocol.
pub mod folia;
/// FOVEA (Flexible Ontology Visual Event Analyzer) protocol.
pub mod fovea;
/// ISO-Space spatial annotation protocol.
pub mod iso_space;
/// LAF/GrAF (Linguistic Annotation Framework / Graph Annotation Framework) protocol.
pub mod laf_graf;
/// NAF (NLP Annotation Format) protocol.
pub mod naf;
/// NIF (NLP Interchange Format) protocol.
pub mod nif;
/// PAULA/Salt/ANNIS multi-layer annotation protocol.
pub mod paula;
/// TEI XML (Text Encoding Initiative) protocol.
pub mod tei;
/// TimeML temporal annotation protocol.
pub mod timeml;
/// UCCA (Universal Conceptual Cognitive Annotation) protocol.
pub mod ucca;
/// UIMA/CAS (Unstructured Information Management Architecture) protocol.
pub mod uima;
/// W3C Web Annotation protocol.
pub mod web_annotation;

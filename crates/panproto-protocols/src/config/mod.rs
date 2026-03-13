//! Configuration format protocol definitions.

/// Ansible playbook schema protocol definition and parser/emitter.
pub mod ansible;
/// AWS CloudFormation protocol definition and parser/emitter.
pub mod cloudformation;
/// HCL/Terraform protocol definition and parser/emitter.
pub mod hcl;
/// Kubernetes CRD protocol definition and parser/emitter.
pub mod k8s_crd;

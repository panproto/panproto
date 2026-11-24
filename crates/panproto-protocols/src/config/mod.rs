//! Configuration format protocol definitions.

/// Ansible playbook, task-file and variables-file protocol definition.
pub mod ansible;
/// AWS `CloudFormation` template protocol definition.
pub mod cloudformation;
/// Kubernetes CRD protocol definition and parser/emitter.
pub mod k8s_crd;

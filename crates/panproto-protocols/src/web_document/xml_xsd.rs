//! XML/XSD protocol definition.
//!
//! XML Schema uses a constrained multigraph + W-type + metadata theory
//! (Group E): schema `colimit(ThGraph, ThConstraint, ThMulti)`,
//! instance `colimit(ThWType, ThMeta)`.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{IndentWriter, children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the XML/XSD protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "xml-xsd".into(),
        schema_theory: "ThXmlXsdSchema".into(),
        instance_theory: "ThXmlXsdInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "element".into(),
            "complex-type".into(),
            "simple-type".into(),
            "attribute".into(),
            "group".into(),
            "sequence".into(),
            "choice".into(),
            "all".into(),
            "restriction".into(),
            "extension".into(),
            "any".into(),
            "annotation".into(),
            "string".into(),
            "integer".into(),
            "boolean".into(),
            "decimal".into(),
            "date".into(),
            "datetime".into(),
            "float".into(),
            "double".into(),
        ],
        constraint_sorts: vec![
            "minOccurs".into(),
            "maxOccurs".into(),
            "use".into(),
            "fixed".into(),
            "default".into(),
            "nillable".into(),
            "pattern".into(),
            "enumeration".into(),
            "minInclusive".into(),
            "maxInclusive".into(),
        ],
    }
}

/// Register the component GATs for XML/XSD with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_multigraph_wtype_meta(registry, "ThXmlXsdSchema", "ThXmlXsdInstance");
}

/// Parse a simplified XSD-like XML schema definition into a [`Schema`].
///
/// Expects a text representation using simplified XSD syntax with element,
/// complexType, simpleType, and attribute declarations.
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_xsd(input: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut counter: usize = 0;

    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        if trimmed.starts_with("<xs:element") || trimmed.starts_with("<xsd:element") {
            let (b, new_i) = parse_xsd_element(builder, &lines, i, "", &mut counter)?;
            builder = b;
            i = new_i;
        } else if trimmed.starts_with("<xs:complexType") || trimmed.starts_with("<xsd:complexType")
        {
            let (b, new_i) = parse_xsd_complex_type(builder, &lines, i, "", &mut counter)?;
            builder = b;
            i = new_i;
        } else if trimmed.starts_with("<xs:simpleType") || trimmed.starts_with("<xsd:simpleType") {
            let (b, new_i) = parse_xsd_simple_type(builder, &lines, i, "", &mut counter)?;
            builder = b;
            i = new_i;
        } else {
            i += 1;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Extract an XML attribute value from a tag string.
fn extract_attr<'a>(tag: &'a str, attr_name: &str) -> Option<&'a str> {
    let search = format!("{attr_name}=\"");
    if let Some(start) = tag.find(&search) {
        let val_start = start + search.len();
        if let Some(end) = tag[val_start..].find('"') {
            return Some(&tag[val_start..val_start + end]);
        }
    }
    None
}

/// Parse an xs:element declaration.
fn parse_xsd_element(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    prefix: &str,
    counter: &mut usize,
) -> Result<(SchemaBuilder, usize), ProtocolError> {
    let trimmed = lines[start].trim();
    let name = extract_attr(trimmed, "name").unwrap_or_else(|| {
        *counter += 1;
        "element"
    });
    let elem_id = if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    };

    let type_attr = extract_attr(trimmed, "type");
    let kind = type_attr.map_or("element", xsd_type_to_kind);
    builder = builder.vertex(&elem_id, kind, None)?;

    // Add constraints from attributes.
    if let Some(v) = extract_attr(trimmed, "minOccurs") {
        builder = builder.constraint(&elem_id, "minOccurs", v);
    }
    if let Some(v) = extract_attr(trimmed, "maxOccurs") {
        builder = builder.constraint(&elem_id, "maxOccurs", v);
    }
    if let Some(v) = extract_attr(trimmed, "nillable") {
        builder = builder.constraint(&elem_id, "nillable", v);
    }
    if let Some(v) = extract_attr(trimmed, "fixed") {
        builder = builder.constraint(&elem_id, "fixed", v);
    }
    if let Some(v) = extract_attr(trimmed, "default") {
        builder = builder.constraint(&elem_id, "default", v);
    }

    // Self-closing tag.
    if trimmed.ends_with("/>") {
        return Ok((builder, start + 1));
    }

    // Walk children until closing tag.
    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with("</xs:element") || line.starts_with("</xsd:element") {
            return Ok((builder, i + 1));
        }
        if line.starts_with("<xs:complexType") || line.starts_with("<xsd:complexType") {
            let (b, new_i) = parse_xsd_complex_type(builder, lines, i, &elem_id, counter)?;
            builder = b;
            i = new_i;
        } else if line.starts_with("<xs:simpleType") || line.starts_with("<xsd:simpleType") {
            let (b, new_i) = parse_xsd_simple_type(builder, lines, i, &elem_id, counter)?;
            builder = b;
            i = new_i;
        } else if line.starts_with("<xs:element") || line.starts_with("<xsd:element") {
            let (b, new_i) = parse_xsd_element(builder, lines, i, &elem_id, counter)?;
            builder = b;
            i = new_i;
        } else {
            i += 1;
        }
    }

    Ok((builder, i))
}

/// Parse an xs:complexType declaration.
fn parse_xsd_complex_type(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    prefix: &str,
    counter: &mut usize,
) -> Result<(SchemaBuilder, usize), ProtocolError> {
    let trimmed = lines[start].trim();
    let name = extract_attr(trimmed, "name");
    let ct_id = if let Some(n) = name {
        if prefix.is_empty() {
            n.to_string()
        } else {
            format!("{prefix}.{n}")
        }
    } else {
        *counter += 1;
        if prefix.is_empty() {
            format!("complexType{counter}")
        } else {
            format!("{prefix}:complexType{counter}")
        }
    };

    builder = builder.vertex(&ct_id, "complex-type", None)?;
    if !prefix.is_empty() {
        builder = builder.edge(prefix, &ct_id, "prop", name)?;
    }

    if trimmed.ends_with("/>") {
        return Ok((builder, start + 1));
    }

    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with("</xs:complexType") || line.starts_with("</xsd:complexType") {
            return Ok((builder, i + 1));
        }
        if line.starts_with("<xs:sequence") || line.starts_with("<xsd:sequence") {
            let (b, new_i) = parse_xsd_compositor(builder, lines, i, &ct_id, "sequence", counter)?;
            builder = b;
            i = new_i;
        } else if line.starts_with("<xs:choice") || line.starts_with("<xsd:choice") {
            let (b, new_i) = parse_xsd_compositor(builder, lines, i, &ct_id, "choice", counter)?;
            builder = b;
            i = new_i;
        } else if line.starts_with("<xs:all") || line.starts_with("<xsd:all") {
            let (b, new_i) = parse_xsd_compositor(builder, lines, i, &ct_id, "all", counter)?;
            builder = b;
            i = new_i;
        } else if line.starts_with("<xs:attribute") || line.starts_with("<xsd:attribute") {
            let (b, new_i) = parse_xsd_attribute(builder, lines, i, &ct_id, counter)?;
            builder = b;
            i = new_i;
        } else if line.starts_with("<xs:element") || line.starts_with("<xsd:element") {
            let (b, new_i) = parse_xsd_element(builder, lines, i, &ct_id, counter)?;
            builder = b;
            i = new_i;
        } else {
            i += 1;
        }
    }

    Ok((builder, i))
}

/// Parse xs:sequence, xs:choice, or xs:all compositors.
fn parse_xsd_compositor(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    parent_id: &str,
    compositor_kind: &str,
    counter: &mut usize,
) -> Result<(SchemaBuilder, usize), ProtocolError> {
    *counter += 1;
    let comp_id = format!("{parent_id}:{compositor_kind}{counter}");
    builder = builder.vertex(&comp_id, compositor_kind, None)?;
    builder = builder.edge(parent_id, &comp_id, "items", Some(compositor_kind))?;

    let trimmed = lines[start].trim();
    if trimmed.ends_with("/>") {
        return Ok((builder, start + 1));
    }

    let close_tag = format!("</xs:{compositor_kind}");
    let close_tag2 = format!("</xsd:{compositor_kind}");

    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with(&close_tag) || line.starts_with(&close_tag2) {
            return Ok((builder, i + 1));
        }
        if line.starts_with("<xs:element") || line.starts_with("<xsd:element") {
            let (b, new_i) = parse_xsd_element(builder, lines, i, &comp_id, counter)?;
            builder = b;
            i = new_i;
        } else if line.starts_with("<xs:choice") || line.starts_with("<xsd:choice") {
            let (b, new_i) = parse_xsd_compositor(builder, lines, i, &comp_id, "choice", counter)?;
            builder = b;
            i = new_i;
        } else if line.starts_with("<xs:sequence") || line.starts_with("<xsd:sequence") {
            let (b, new_i) =
                parse_xsd_compositor(builder, lines, i, &comp_id, "sequence", counter)?;
            builder = b;
            i = new_i;
        } else {
            i += 1;
        }
    }

    Ok((builder, i))
}

/// Parse an xs:attribute declaration.
fn parse_xsd_attribute(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    parent_id: &str,
    counter: &mut usize,
) -> Result<(SchemaBuilder, usize), ProtocolError> {
    let trimmed = lines[start].trim();
    let name = extract_attr(trimmed, "name").unwrap_or_else(|| {
        *counter += 1;
        "attr"
    });
    let attr_id = format!("{parent_id}.{name}");
    builder = builder.vertex(&attr_id, "attribute", None)?;
    builder = builder.edge(parent_id, &attr_id, "prop", Some(name))?;

    if let Some(v) = extract_attr(trimmed, "use") {
        builder = builder.constraint(&attr_id, "use", v);
    }
    if let Some(v) = extract_attr(trimmed, "fixed") {
        builder = builder.constraint(&attr_id, "fixed", v);
    }
    if let Some(v) = extract_attr(trimmed, "default") {
        builder = builder.constraint(&attr_id, "default", v);
    }

    if trimmed.ends_with("/>") {
        return Ok((builder, start + 1));
    }

    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with("</xs:attribute") || line.starts_with("</xsd:attribute") {
            return Ok((builder, i + 1));
        }
        if line.starts_with("<xs:simpleType") || line.starts_with("<xsd:simpleType") {
            let (b, new_i) = parse_xsd_simple_type(builder, lines, i, &attr_id, counter)?;
            builder = b;
            i = new_i;
        } else {
            i += 1;
        }
    }

    Ok((builder, i))
}

/// Parse an xs:simpleType declaration.
fn parse_xsd_simple_type(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    prefix: &str,
    counter: &mut usize,
) -> Result<(SchemaBuilder, usize), ProtocolError> {
    let trimmed = lines[start].trim();
    let name = extract_attr(trimmed, "name");
    let st_id = if let Some(n) = name {
        if prefix.is_empty() {
            n.to_string()
        } else {
            format!("{prefix}.{n}")
        }
    } else {
        *counter += 1;
        if prefix.is_empty() {
            format!("simpleType{counter}")
        } else {
            format!("{prefix}:simpleType{counter}")
        }
    };

    builder = builder.vertex(&st_id, "simple-type", None)?;
    if !prefix.is_empty() {
        builder = builder.edge(prefix, &st_id, "prop", name)?;
    }

    if trimmed.ends_with("/>") {
        return Ok((builder, start + 1));
    }

    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with("</xs:simpleType") || line.starts_with("</xsd:simpleType") {
            return Ok((builder, i + 1));
        }
        if line.starts_with("<xs:restriction") || line.starts_with("<xsd:restriction") {
            let (b, new_i) = parse_xsd_restriction(builder, lines, i, &st_id, counter)?;
            builder = b;
            i = new_i;
        } else {
            i += 1;
        }
    }

    Ok((builder, i))
}

/// Parse an xs:restriction block.
fn parse_xsd_restriction(
    mut builder: SchemaBuilder,
    lines: &[&str],
    start: usize,
    parent_id: &str,
    counter: &mut usize,
) -> Result<(SchemaBuilder, usize), ProtocolError> {
    *counter += 1;
    let rest_id = format!("{parent_id}:restriction{counter}");
    builder = builder.vertex(&rest_id, "restriction", None)?;
    builder = builder.edge(parent_id, &rest_id, "variant", Some("restriction"))?;

    let trimmed = lines[start].trim();
    if trimmed.ends_with("/>") {
        return Ok((builder, start + 1));
    }

    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with("</xs:restriction") || line.starts_with("</xsd:restriction") {
            return Ok((builder, i + 1));
        }
        // Pick up enumeration, pattern, minInclusive, maxInclusive facets.
        if line.starts_with("<xs:enumeration") || line.starts_with("<xsd:enumeration") {
            if let Some(v) = extract_attr(line, "value") {
                builder = builder.constraint(&rest_id, "enumeration", v);
            }
        } else if line.starts_with("<xs:pattern") || line.starts_with("<xsd:pattern") {
            if let Some(v) = extract_attr(line, "value") {
                builder = builder.constraint(&rest_id, "pattern", v);
            }
        } else if line.starts_with("<xs:minInclusive") || line.starts_with("<xsd:minInclusive") {
            if let Some(v) = extract_attr(line, "value") {
                builder = builder.constraint(&rest_id, "minInclusive", v);
            }
        } else if line.starts_with("<xs:maxInclusive") || line.starts_with("<xsd:maxInclusive") {
            if let Some(v) = extract_attr(line, "value") {
                builder = builder.constraint(&rest_id, "maxInclusive", v);
            }
        }
        i += 1;
    }

    Ok((builder, i))
}

/// Map XSD type to vertex kind.
fn xsd_type_to_kind(type_str: &str) -> &'static str {
    let base = type_str.split(':').next_back().unwrap_or(type_str);
    match base {
        "string" | "normalizedString" | "token" | "Name" | "NCName" | "NMTOKEN" => "string",
        "integer" | "int" | "long" | "short" | "byte" | "nonNegativeInteger"
        | "positiveInteger" | "negativeInteger" | "nonPositiveInteger" | "unsignedInt"
        | "unsignedLong" | "unsignedShort" | "unsignedByte" => "integer",
        "boolean" => "boolean",
        "decimal" => "decimal",
        "date" | "gYear" | "gMonth" | "gDay" | "gYearMonth" | "gMonthDay" => "date",
        "dateTime" => "datetime",
        "float" => "float",
        "double" => "double",
        _ => "element",
    }
}

/// Map vertex kind back to XSD type.
fn kind_to_xsd_type(kind: &str) -> &'static str {
    match kind {
        "string" => "xs:string",
        "integer" => "xs:integer",
        "boolean" => "xs:boolean",
        "decimal" => "xs:decimal",
        "date" => "xs:date",
        "datetime" => "xs:dateTime",
        "float" => "xs:float",
        "double" => "xs:double",
        _ => "xs:string",
    }
}

/// Emit a [`Schema`] as simplified XSD text.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_xsd(schema: &Schema) -> Result<String, ProtocolError> {
    let structural = &["prop", "items", "variant", "ref"];
    let roots = find_roots(schema, structural);

    let mut w = IndentWriter::new("  ");
    w.line("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    w.line("<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\">");
    w.indent();

    for root in &roots {
        emit_xsd_vertex(schema, root, &mut w);
    }

    w.dedent();
    w.line("</xs:schema>");
    Ok(w.finish())
}

/// Emit a single vertex as XSD.
fn emit_xsd_vertex(schema: &Schema, vertex: &panproto_schema::Vertex, w: &mut IndentWriter) {
    let name = vertex.id.rsplit('.').next().unwrap_or(&vertex.id);
    match vertex.kind.as_str() {
        "element" => {
            let children = children_by_edge(schema, &vertex.id, "prop");
            let items = children_by_edge(schema, &vertex.id, "items");
            if children.is_empty() && items.is_empty() {
                w.line(&format!(
                    "<xs:element name=\"{name}\" type=\"{}\"/>",
                    kind_to_xsd_type(&vertex.kind)
                ));
            } else {
                w.line(&format!("<xs:element name=\"{name}\">"));
                w.indent();
                for (_, child) in &children {
                    emit_xsd_vertex(schema, child, w);
                }
                for (_, child) in &items {
                    emit_xsd_vertex(schema, child, w);
                }
                w.dedent();
                w.line("</xs:element>");
            }
        }
        "complex-type" => {
            w.line(&format!("<xs:complexType name=\"{name}\">"));
            w.indent();
            let items = children_by_edge(schema, &vertex.id, "items");
            for (_, child) in &items {
                emit_xsd_vertex(schema, child, w);
            }
            let props = children_by_edge(schema, &vertex.id, "prop");
            for (_, child) in &props {
                emit_xsd_vertex(schema, child, w);
            }
            w.dedent();
            w.line("</xs:complexType>");
        }
        "simple-type" => {
            w.line(&format!("<xs:simpleType name=\"{name}\">"));
            w.indent();
            let variants = children_by_edge(schema, &vertex.id, "variant");
            for (_, child) in &variants {
                emit_xsd_vertex(schema, child, w);
            }
            w.dedent();
            w.line("</xs:simpleType>");
        }
        "attribute" => {
            let use_val = constraint_value(schema, &vertex.id, "use").unwrap_or("optional");
            w.line(&format!(
                "<xs:attribute name=\"{name}\" use=\"{use_val}\"/>"
            ));
        }
        "sequence" => {
            w.line("<xs:sequence>");
            w.indent();
            let children = children_by_edge(schema, &vertex.id, "prop");
            for (_, child) in &children {
                emit_xsd_vertex(schema, child, w);
            }
            w.dedent();
            w.line("</xs:sequence>");
        }
        "choice" => {
            w.line("<xs:choice>");
            w.indent();
            let children = children_by_edge(schema, &vertex.id, "prop");
            for (_, child) in &children {
                emit_xsd_vertex(schema, child, w);
            }
            w.dedent();
            w.line("</xs:choice>");
        }
        "restriction" => {
            w.line("<xs:restriction base=\"xs:string\">");
            w.indent();
            if let Some(v) = constraint_value(schema, &vertex.id, "pattern") {
                w.line(&format!("<xs:pattern value=\"{v}\"/>"));
            }
            if let Some(v) = constraint_value(schema, &vertex.id, "enumeration") {
                w.line(&format!("<xs:enumeration value=\"{v}\"/>"));
            }
            w.dedent();
            w.line("</xs:restriction>");
        }
        _ => {
            w.line(&format!(
                "<xs:element name=\"{name}\" type=\"{}\"/>",
                kind_to_xsd_type(&vertex.kind)
            ));
        }
    }
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec![
                "element".into(),
                "complex-type".into(),
                "sequence".into(),
                "choice".into(),
                "all".into(),
            ],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec!["complex-type".into(), "element".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "variant".into(),
            src_kinds: vec!["simple-type".into(), "element".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "ref".into(),
            src_kinds: vec![],
            tgt_kinds: vec![],
        },
    ]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_def() {
        let p = protocol();
        assert_eq!(p.name, "xml-xsd");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThXmlXsdSchema"));
        assert!(registry.contains_key("ThXmlXsdInstance"));
    }

    #[test]
    fn parse_simple_xsd() {
        let xsd = r#"
<xs:element name="person" type="xs:string"/>
<xs:complexType name="Address">
  <xs:sequence>
    <xs:element name="street" type="xs:string"/>
    <xs:element name="city" type="xs:string"/>
  </xs:sequence>
  <xs:attribute name="country" use="required"/>
</xs:complexType>
"#;
        let schema = parse_xsd(xsd).expect("should parse");
        assert!(schema.has_vertex("person"));
        assert!(schema.has_vertex("Address"));
    }

    #[test]
    fn roundtrip() {
        let xsd = r#"
<xs:element name="root" type="xs:string"/>
<xs:complexType name="MyType">
  <xs:attribute name="id" use="required"/>
</xs:complexType>
"#;
        let schema = parse_xsd(xsd).expect("parse");
        let emitted = emit_xsd(&schema).expect("emit");
        assert!(emitted.contains("xs:schema"));
        assert!(emitted.contains("root"));
    }
}

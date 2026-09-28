//! Protobuf protocol definition.
//!
//! Protobuf uses a simple constrained graph schema theory
//! (`ThSimpleGraph + ThConstraint`) and a flat instance theory (`ThFlat`).
//!
//! Vertex kinds: message, field, enum, enum-value, oneof, service, rpc, map.
//! Edge kinds: field-of, type-of, variant-of, input-of, output-of.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::error::ProtocolError;
use crate::theories;

/// Returns the Protobuf protocol definition.
///
/// Schema theory: `ThSimpleGraph + ThConstraint`.
/// Instance theory: `ThFlat`.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "protobuf".into(),
        schema_theory: "ThProtobufSchema".into(),
        instance_theory: "ThProtobufInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "message".into(),
            "field".into(),
            "enum".into(),
            "enum-value".into(),
            "oneof".into(),
            "service".into(),
            "rpc".into(),
            "map".into(),
            "string".into(),
            "integer".into(),
            "float".into(),
            "boolean".into(),
        ],
        constraint_sorts: vec!["field_number".into(), "label".into(), "packed".into()],
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for Protobuf with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_simple_graph_flat(registry, "ThProtobufSchema", "ThProtobufInstance");
}

/// A reference from a field, map value or rpc to a named type, resolved
/// once every declaration in the file is known.
struct FieldInfo {
    /// The vertex the `type-of` edge leaves.
    field_id: String,
    /// The type name as written: relative (`Inner`, `Outer.Inner`) or
    /// fully qualified (`.pkg.Outer`).
    type_name: String,
    /// The id of the message the reference is written in, empty at file
    /// level. Relative names resolve from here outward.
    scope: String,
}

/// A lexical token of a `.proto` file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tok<'a> {
    /// An identifier, dotted name, keyword or number literal.
    Word(&'a str),
    /// A string literal (its contents are never needed).
    Str,
    /// A punctuation character.
    Sym(u8),
}

/// Whether a byte continues a word: identifier and number characters,
/// the `.` of dotted and fully qualified names, and any non-ASCII byte.
const fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b >= 0x80
}

/// Split `.proto` source into tokens, dropping whitespace, `//` line
/// comments and `/* */` block comments.
fn tokenize(src: &str) -> Result<Vec<Tok<'_>>, ProtocolError> {
    let bytes = src.as_bytes();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
        } else if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if b == b'/' && bytes.get(i + 1) == Some(&b'*') {
            let end = src[i + 2..]
                .find("*/")
                .ok_or_else(|| ProtocolError::Parse("unterminated block comment".into()))?;
            i += 2 + end + 2;
        } else if b == b'"' || b == b'\'' {
            i += 1;
            loop {
                match bytes.get(i) {
                    None => {
                        return Err(ProtocolError::Parse("unterminated string literal".into()));
                    }
                    Some(b'\\') => i += 2,
                    Some(&c) if c == b => {
                        i += 1;
                        break;
                    }
                    Some(_) => i += 1,
                }
            }
            toks.push(Tok::Str);
        } else if is_word_byte(b) {
            let start = i;
            while i < bytes.len() && is_word_byte(bytes[i]) {
                i += 1;
            }
            toks.push(Tok::Word(&src[start..i]));
        } else {
            toks.push(Tok::Sym(b));
            i += 1;
        }
    }
    Ok(toks)
}

/// A recursive-descent reader of a tokenized `.proto` file.
///
/// It reads the declarations the schema records (messages, enums,
/// oneofs, fields, map fields, groups, services and rpcs) and steps over
/// every other statement (`syntax`, `edition`, `import`, `option`,
/// `reserved`, `extensions`, `extend`, and field options), with brackets
/// balanced, so option values spanning lines or holding message literals
/// are skipped whole.
struct ProtoReader<'a> {
    toks: Vec<Tok<'a>>,
    pos: usize,
    /// The `package` declaration, empty when there is none.
    package: String,
    /// The ids of the message and enum vertices, which type references
    /// resolve against.
    type_ids: std::collections::HashSet<String>,
    /// The type references to resolve once every declaration is read.
    refs: Vec<FieldInfo>,
}

impl<'a> ProtoReader<'a> {
    fn peek(&self) -> Option<Tok<'a>> {
        self.toks.get(self.pos).copied()
    }

    fn peek_word(&self) -> Option<&'a str> {
        match self.peek() {
            Some(Tok::Word(w)) => Some(w),
            _ => None,
        }
    }

    fn eat_sym(&mut self, c: u8) -> bool {
        if self.peek() == Some(Tok::Sym(c)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_sym(&mut self, c: u8, context: &str) -> Result<(), ProtocolError> {
        if self.eat_sym(c) {
            Ok(())
        } else {
            Err(self.unexpected(&format!("'{}' {context}", char::from(c))))
        }
    }

    fn word(&mut self, context: &str) -> Result<&'a str, ProtocolError> {
        if let Some(w) = self.peek_word() {
            self.pos += 1;
            Ok(w)
        } else {
            Err(self.unexpected(context))
        }
    }

    fn unexpected(&self, wanted: &str) -> ProtocolError {
        let found = match self.peek() {
            None => "end of file".to_owned(),
            Some(Tok::Word(w)) => format!("'{w}'"),
            Some(Tok::Str) => "a string literal".to_owned(),
            Some(Tok::Sym(c)) => format!("'{}'", char::from(c)),
        };
        ProtocolError::Parse(format!(
            "expected {wanted}, found {found} (token {})",
            self.pos
        ))
    }

    /// Step over one statement: up to and including its `;` at bracket
    /// depth zero, or its balanced `{ ... }` block and a `;` after it.
    /// A `}` at depth zero closes the enclosing block and is left for
    /// the caller.
    fn skip_statement(&mut self) {
        let mut depth = 0usize;
        while let Some(tok) = self.peek() {
            match tok {
                Tok::Sym(b'{' | b'[' | b'(') => depth += 1,
                Tok::Sym(b'}') if depth == 0 => return,
                Tok::Sym(b'}' | b']' | b')') => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 && tok == Tok::Sym(b'}') {
                        self.pos += 1;
                        self.eat_sym(b';');
                        return;
                    }
                }
                Tok::Sym(b';') if depth == 0 => {
                    self.pos += 1;
                    return;
                }
                _ => {}
            }
            self.pos += 1;
        }
    }

    /// Read the whole file.
    fn file(&mut self, mut builder: SchemaBuilder) -> Result<SchemaBuilder, ProtocolError> {
        while let Some(tok) = self.peek() {
            match tok {
                Tok::Word("message") => {
                    self.pos += 1;
                    builder = self.message(builder, "")?;
                }
                Tok::Word("enum") => {
                    self.pos += 1;
                    builder = self.enumeration(builder, "")?;
                }
                Tok::Word("service") => {
                    self.pos += 1;
                    builder = self.service(builder)?;
                }
                Tok::Word("package") => {
                    self.pos += 1;
                    self.package = self.word("a package name")?.to_owned();
                    self.skip_statement();
                }
                Tok::Sym(b'}') => return Err(self.unexpected("a declaration")),
                _ => self.skip_statement(),
            }
        }
        Ok(builder)
    }

    /// Read `message Name { ... }` from after the keyword, declaring it
    /// under `scope`.
    fn message(
        &mut self,
        mut builder: SchemaBuilder,
        scope: &str,
    ) -> Result<SchemaBuilder, ProtocolError> {
        let name = self.word("a message name")?;
        let msg_id = scoped(scope, name);
        builder = builder.vertex(&msg_id, "message", None)?;
        self.type_ids.insert(msg_id.clone());
        self.message_body(builder, &msg_id)
    }

    /// Read a message body `{ ... }` for the message `msg_id`.
    fn message_body(
        &mut self,
        mut builder: SchemaBuilder,
        msg_id: &str,
    ) -> Result<SchemaBuilder, ProtocolError> {
        self.expect_sym(b'{', "to open a message body")?;
        loop {
            match self.peek() {
                None => return Err(self.unexpected(&format!("'}}' to close message {msg_id}"))),
                Some(Tok::Sym(b'}')) => {
                    self.pos += 1;
                    return Ok(builder);
                }
                Some(Tok::Word("message")) => {
                    self.pos += 1;
                    builder = self.message(builder, msg_id)?;
                }
                Some(Tok::Word("enum")) => {
                    self.pos += 1;
                    builder = self.enumeration(builder, msg_id)?;
                }
                Some(Tok::Word("oneof")) => {
                    self.pos += 1;
                    builder = self.oneof(builder, msg_id)?;
                }
                Some(
                    Tok::Word("option" | "reserved" | "extensions" | "extend") | Tok::Sym(b';'),
                ) => {
                    self.skip_statement();
                }
                Some(_) => builder = self.field(builder, msg_id, msg_id)?,
            }
        }
    }

    /// Read one field declaration into `parent_id` (a message or oneof),
    /// written inside the message `scope`.
    ///
    /// `[label] Type name = N [options];`, a `map<K, V> name = N;`, or a
    /// proto2 `[label] group Name = N { ... }`.
    fn field(
        &mut self,
        mut builder: SchemaBuilder,
        parent_id: &str,
        scope: &str,
    ) -> Result<SchemaBuilder, ProtocolError> {
        let label = match self.peek_word() {
            Some(l @ ("required" | "optional" | "repeated")) => {
                self.pos += 1;
                Some(l)
            }
            _ => None,
        };
        let type_name = self.word("a field type")?;
        if type_name == "map" && self.peek() == Some(Tok::Sym(b'<')) {
            return self.map_field(builder, parent_id, scope);
        }
        if type_name == "group" && self.toks.get(self.pos + 1) == Some(&Tok::Sym(b'=')) {
            return self.group(builder, parent_id, scope, label);
        }
        let field_name = self.word("a field name")?;
        self.expect_sym(b'=', &format!("after field {field_name}"))?;
        let number = self.word("a field number")?;
        self.skip_statement();

        let field_id = format!("{parent_id}.{field_name}");
        builder = builder.vertex(&field_id, "field", None)?;
        builder = builder.edge(parent_id, &field_id, "field-of", Some(field_name))?;
        builder = builder.constraint(&field_id, "field_number", number);
        if let Some(l) = label {
            builder = builder.constraint(&field_id, "label", l);
        }
        self.refs.push(FieldInfo {
            field_id,
            type_name: type_name.to_owned(),
            scope: scope.to_owned(),
        });
        Ok(builder)
    }

    /// Read `map<K, V> name = N [options];` from after `map`.
    ///
    /// The map field has a `key` and a `value` vertex of the entry
    /// types' kinds. A value of message or enum type refers to it.
    fn map_field(
        &mut self,
        mut builder: SchemaBuilder,
        parent_id: &str,
        scope: &str,
    ) -> Result<SchemaBuilder, ProtocolError> {
        self.expect_sym(b'<', "after map")?;
        let key_type = self.word("a map key type")?;
        self.expect_sym(b',', "between map key and value types")?;
        let val_type = self.word("a map value type")?;
        self.expect_sym(b'>', "to close a map type")?;
        let field_name = self.word("a map field name")?;
        self.expect_sym(b'=', &format!("after map field {field_name}"))?;
        let number = self.word("a field number")?;
        self.skip_statement();

        let field_id = format!("{parent_id}.{field_name}");
        builder = builder.vertex(&field_id, "map", None)?;
        builder = builder.edge(parent_id, &field_id, "field-of", Some(field_name))?;
        builder = builder.constraint(&field_id, "field_number", number);

        let key_id = format!("{field_id}:key");
        builder = builder.vertex(&key_id, proto_scalar_kind(key_type), None)?;
        builder = builder.edge(&field_id, &key_id, "type-of", Some("key"))?;

        let val_id = format!("{field_id}:value");
        let val_kind = proto_scalar_kind(val_type);
        builder = builder.vertex(&val_id, val_kind, None)?;
        builder = builder.edge(&field_id, &val_id, "type-of", Some("value"))?;
        if val_kind == "field" {
            self.refs.push(FieldInfo {
                field_id: val_id,
                type_name: val_type.to_owned(),
                scope: scope.to_owned(),
            });
        }
        Ok(builder)
    }

    /// Read a proto2 `group Name = N [options] { ... }` from after
    /// `group`: a nested message `Name` and a field named `Name` in
    /// lower case whose type it is.
    fn group(
        &mut self,
        mut builder: SchemaBuilder,
        parent_id: &str,
        scope: &str,
        label: Option<&str>,
    ) -> Result<SchemaBuilder, ProtocolError> {
        let name = self.word("a group name")?;
        self.expect_sym(b'=', &format!("after group {name}"))?;
        let number = self.word("a group field number")?;
        if self.peek() == Some(Tok::Sym(b'[')) {
            let mut depth = 0usize;
            while let Some(tok) = self.peek() {
                self.pos += 1;
                match tok {
                    Tok::Sym(b'[') => depth += 1,
                    Tok::Sym(b']') => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
        let msg_id = scoped(scope, name);
        builder = builder.vertex(&msg_id, "message", None)?;
        self.type_ids.insert(msg_id.clone());
        builder = self.message_body(builder, &msg_id)?;

        let field_name = name.to_ascii_lowercase();
        let field_id = format!("{parent_id}.{field_name}");
        builder = builder.vertex(&field_id, "field", None)?;
        builder = builder.edge(parent_id, &field_id, "field-of", Some(&field_name))?;
        builder = builder.constraint(&field_id, "field_number", number);
        if let Some(l) = label {
            builder = builder.constraint(&field_id, "label", l);
        }
        self.refs.push(FieldInfo {
            field_id,
            type_name: name.to_owned(),
            scope: scope.to_owned(),
        });
        Ok(builder)
    }

    /// Read `enum Name { ... }` from after the keyword, declaring it
    /// under `scope`. Each value is an `enum-value` vertex.
    fn enumeration(
        &mut self,
        mut builder: SchemaBuilder,
        scope: &str,
    ) -> Result<SchemaBuilder, ProtocolError> {
        let name = self.word("an enum name")?;
        let enum_id = scoped(scope, name);
        builder = builder.vertex(&enum_id, "enum", None)?;
        self.type_ids.insert(enum_id.clone());
        self.expect_sym(b'{', &format!("to open enum {enum_id}"))?;
        loop {
            match self.peek() {
                None => return Err(self.unexpected(&format!("'}}' to close enum {enum_id}"))),
                Some(Tok::Sym(b'}')) => {
                    self.pos += 1;
                    return Ok(builder);
                }
                Some(Tok::Word("option" | "reserved") | Tok::Sym(b';')) => self.skip_statement(),
                Some(_) => {
                    let val_name = self.word("an enum value name")?;
                    self.expect_sym(b'=', &format!("after enum value {val_name}"))?;
                    self.skip_statement();
                    let val_id = format!("{enum_id}.{val_name}");
                    builder = builder.vertex(&val_id, "enum-value", None)?;
                    builder = builder.edge(&enum_id, &val_id, "variant-of", Some(val_name))?;
                }
            }
        }
    }

    /// Read `oneof name { ... }` from after the keyword, inside the
    /// message `msg_id`.
    fn oneof(
        &mut self,
        mut builder: SchemaBuilder,
        msg_id: &str,
    ) -> Result<SchemaBuilder, ProtocolError> {
        let name = self.word("a oneof name")?;
        let oneof_id = format!("{msg_id}.{name}");
        builder = builder.vertex(&oneof_id, "oneof", None)?;
        builder = builder.edge(msg_id, &oneof_id, "field-of", Some(name))?;
        self.expect_sym(b'{', &format!("to open oneof {oneof_id}"))?;
        loop {
            match self.peek() {
                None => return Err(self.unexpected(&format!("'}}' to close oneof {oneof_id}"))),
                Some(Tok::Sym(b'}')) => {
                    self.pos += 1;
                    return Ok(builder);
                }
                Some(Tok::Word("option") | Tok::Sym(b';')) => self.skip_statement(),
                Some(_) => builder = self.field(builder, &oneof_id, msg_id)?,
            }
        }
    }

    /// Read `service Name { ... }` from after the keyword. Each rpc is an
    /// `rpc` vertex referring to its request and response types.
    fn service(&mut self, mut builder: SchemaBuilder) -> Result<SchemaBuilder, ProtocolError> {
        let name = self.word("a service name")?;
        builder = builder.vertex(name, "service", None)?;
        self.expect_sym(b'{', &format!("to open service {name}"))?;
        loop {
            match self.peek() {
                None => return Err(self.unexpected(&format!("'}}' to close service {name}"))),
                Some(Tok::Sym(b'}')) => {
                    self.pos += 1;
                    return Ok(builder);
                }
                Some(Tok::Word("rpc")) => {
                    self.pos += 1;
                    let rpc_name = self.word("an rpc name")?;
                    let rpc_id = format!("{name}.{rpc_name}");
                    builder = builder.vertex(&rpc_id, "rpc", None)?;
                    builder = builder.edge(name, &rpc_id, "field-of", Some(rpc_name))?;
                    let input = self.rpc_type(&rpc_id)?;
                    if self.word("'returns'")? != "returns" {
                        return Err(ProtocolError::Parse(format!(
                            "expected 'returns' in rpc {rpc_id}"
                        )));
                    }
                    let output = self.rpc_type(&rpc_id)?;
                    for type_name in [input, output] {
                        self.refs.push(FieldInfo {
                            field_id: rpc_id.clone(),
                            type_name: type_name.to_owned(),
                            scope: String::new(),
                        });
                    }
                    // `;`, or an options block `{ ... }`.
                    self.skip_statement();
                }
                Some(_) => self.skip_statement(),
            }
        }
    }

    /// Read `( [stream] Type )`, returning the type name.
    fn rpc_type(&mut self, rpc_id: &str) -> Result<&'a str, ProtocolError> {
        self.expect_sym(b'(', &format!("in rpc {rpc_id}"))?;
        let mut type_name = self.word("an rpc type")?;
        if type_name == "stream" && self.peek() != Some(Tok::Sym(b')')) {
            type_name = self.word("an rpc type")?;
        }
        self.expect_sym(b')', &format!("in rpc {rpc_id}"))?;
        Ok(type_name)
    }

    /// The vertex id a type reference written in `scope` names, if the
    /// file declares it.
    ///
    /// A fully qualified name (`.pkg.Outer.Inner`) names one type; a
    /// relative name is looked up in `scope`, then in each enclosing
    /// scope out to the package and the file, and the innermost
    /// declaration wins, as `protoc` resolves it. Vertex ids leave the
    /// package out, so a name resolves only to a type of this package.
    fn resolve(&self, type_name: &str, scope: &str) -> Option<String> {
        let local = |full: &str| -> Option<String> {
            let id = if self.package.is_empty() {
                full
            } else {
                full.strip_prefix(self.package.as_str())?
                    .strip_prefix('.')?
            };
            self.type_ids.contains(id).then(|| id.to_owned())
        };
        if let Some(full) = type_name.strip_prefix('.') {
            return local(full);
        }
        let mut enclosing: Vec<&str> = self
            .package
            .split('.')
            .chain(scope.split('.'))
            .filter(|s| !s.is_empty())
            .collect();
        loop {
            let candidate = if enclosing.is_empty() {
                type_name.to_owned()
            } else {
                format!("{}.{type_name}", enclosing.join("."))
            };
            if let Some(id) = local(&candidate) {
                return Some(id);
            }
            enclosing.pop()?;
        }
    }
}

/// The id of a declaration `name` inside `scope` (empty at file level).
fn scoped(scope: &str, name: &str) -> String {
    if scope.is_empty() {
        name.to_owned()
    } else {
        format!("{scope}.{name}")
    }
}

/// Parse a `.proto` file into a [`Schema`].
///
/// The file is tokenized, so comments, multi-line statements and option
/// values holding message literals read the same wherever they fall.
/// Declarations are read first; each field, map value and rpc type that
/// names a message or enum of the file then gets a `type-of` edge to it,
/// with relative names resolved from the enclosing scope outward.
///
/// # Errors
///
/// Returns [`ProtocolError::Parse`] for a token the grammar does not
/// admit where it appears, or an unterminated block, comment or string;
/// or the schema builder's error for a declaration it rejects, such as
/// two declarations of one name.
pub fn parse_proto(proto: &str) -> Result<Schema, ProtocolError> {
    let proto_def = protocol();
    let mut reader = ProtoReader {
        toks: tokenize(proto)?,
        pos: 0,
        package: String::new(),
        type_ids: std::collections::HashSet::new(),
        refs: Vec::new(),
    };
    let mut builder = reader.file(SchemaBuilder::new(&proto_def))?;

    for info in &reader.refs {
        if let Some(target) = reader.resolve(&info.type_name, &info.scope) {
            builder = builder.edge(&info.field_id, &target, "type-of", None)?;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Map a protobuf scalar type to a vertex kind.
///
/// Maps well-known protobuf scalar types to semantic vertex kinds:
/// string/bytes to `"string"`, integer types to `"integer"`, floating
/// point types to `"float"`, bool to `"boolean"`. User-defined
/// message/enum types fall back to `"field"`.
fn proto_scalar_kind(type_name: &str) -> &'static str {
    match type_name {
        "string" | "bytes" => "string",
        "int32" | "int64" | "uint32" | "uint64" | "sint32" | "sint64" | "fixed32" | "fixed64"
        | "sfixed32" | "sfixed64" => "integer",
        "float" | "double" => "float",
        "bool" => "boolean",
        _ => "field",
    }
}

/// Map a vertex kind back to a protobuf scalar type name.
fn kind_to_proto_scalar(kind: &str) -> &'static str {
    match kind {
        "integer" => "int32",
        "float" => "double",
        "boolean" => "bool",
        _ => "string",
    }
}

/// Emit a [`Schema`] as a `.proto` format string.
///
/// Reconstructs messages, enums, and services from the schema graph,
/// producing a valid proto3 file with `syntax = "proto3";` header.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_proto(schema: &Schema) -> Result<String, ProtocolError> {
    use crate::emit::{IndentWriter, find_roots};

    let structural = &["field-of", "variant-of"];
    let roots = find_roots(schema, structural);

    let mut w = IndentWriter::new("  ");
    w.line("syntax = \"proto3\";");
    w.blank();

    for root in &roots {
        match root.kind.as_str() {
            "message" => emit_proto_message(schema, root, &mut w),
            "enum" => emit_proto_enum(schema, root, &mut w),
            "service" => emit_proto_service(schema, root, &mut w),
            _ => {}
        }
    }

    Ok(w.finish())
}

/// Emit a single message declaration.
fn emit_proto_message(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut crate::emit::IndentWriter,
) {
    use crate::emit::{children_by_edge, constraint_value};

    // Use the short name (last segment after dot).
    let name = vertex.id.rsplit('.').next().unwrap_or(&vertex.id);
    w.line(&format!("message {name} {{"));
    w.indent();

    let fields = children_by_edge(schema, &vertex.id, "field-of");
    for (edge, field_vertex) in &fields {
        let field_name = edge.name.as_deref().unwrap_or(&field_vertex.id);

        match field_vertex.kind.as_str() {
            "oneof" => {
                w.line(&format!("oneof {field_name} {{"));
                w.indent();
                let oneof_fields = children_by_edge(schema, &field_vertex.id, "field-of");
                for (oe, ov) in &oneof_fields {
                    let of_name = oe.name.as_deref().unwrap_or(&ov.id);
                    let type_name = resolve_proto_field_type(schema, ov);
                    let num = constraint_value(schema, &ov.id, "field_number").unwrap_or("0");
                    w.line(&format!("{type_name} {of_name} = {num};"));
                }
                w.dedent();
                w.line("}");
            }
            "map" => {
                let num = constraint_value(schema, &field_vertex.id, "field_number").unwrap_or("0");
                let type_of_edges = children_by_edge(schema, &field_vertex.id, "type-of");
                let mut key_type = "string";
                let mut val_type = "string";
                for (te, tv) in &type_of_edges {
                    let edge_name = te.name.as_deref().unwrap_or("");
                    if edge_name == "key" {
                        key_type = kind_to_proto_scalar(&tv.kind);
                    } else if edge_name == "value" {
                        val_type = kind_to_proto_scalar(&tv.kind);
                    }
                }
                w.line(&format!(
                    "map<{key_type}, {val_type}> {field_name} = {num};"
                ));
            }
            "field" => {
                let label = constraint_value(schema, &field_vertex.id, "label").unwrap_or("");
                let num = constraint_value(schema, &field_vertex.id, "field_number").unwrap_or("0");
                let type_name = resolve_proto_field_type(schema, field_vertex);
                let label_prefix = if label.is_empty() {
                    String::new()
                } else {
                    format!("{label} ")
                };
                w.line(&format!("{label_prefix}{type_name} {field_name} = {num};"));
            }
            _ => {
                // Nested message or enum handled as child vertex.
                let num = constraint_value(schema, &field_vertex.id, "field_number").unwrap_or("0");
                let type_name = resolve_proto_field_type(schema, field_vertex);
                w.line(&format!("{type_name} {field_name} = {num};"));
            }
        }
    }

    // Check for nested messages/enums.
    for (_, child) in children_by_edge(schema, &vertex.id, "field-of") {
        if child.kind == "message" {
            emit_proto_message(schema, child, w);
        } else if child.kind == "enum" {
            emit_proto_enum(schema, child, w);
        }
    }

    w.dedent();
    w.line("}");
    w.blank();
}

/// Resolve the type name for a protobuf field.
fn resolve_proto_field_type(schema: &Schema, field_vertex: &panproto_schema::Vertex) -> String {
    use crate::emit::children_by_edge;

    // The field's declared type is the target of its single `type-of` edge,
    // if any; a field with none is a scalar named by its own kind.
    children_by_edge(schema, &field_vertex.id, "type-of")
        .first()
        .map(|(_, v)| *v)
        .map_or_else(
            || kind_to_proto_scalar(&field_vertex.kind).to_string(),
            |type_vertex| match type_vertex.kind.as_str() {
                "message" | "enum" => type_vertex
                    .id
                    .rsplit('.')
                    .next()
                    .unwrap_or(&type_vertex.id)
                    .to_string(),
                other => kind_to_proto_scalar(other).to_string(),
            },
        )
}

/// Emit a single enum declaration.
fn emit_proto_enum(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut crate::emit::IndentWriter,
) {
    use crate::emit::children_by_edge;

    let name = vertex.id.rsplit('.').next().unwrap_or(&vertex.id);
    w.line(&format!("enum {name} {{"));
    w.indent();

    let variants = children_by_edge(schema, &vertex.id, "variant-of");
    for (edge, _variant_vertex) in &variants {
        let val_name = edge.name.as_deref().unwrap_or("UNKNOWN");
        w.line(&format!("{val_name} = 0;"));
    }

    w.dedent();
    w.line("}");
    w.blank();
}

/// Emit a single service declaration.
fn emit_proto_service(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
    w: &mut crate::emit::IndentWriter,
) {
    use crate::emit::children_by_edge;

    let name = vertex.id.rsplit('.').next().unwrap_or(&vertex.id);
    w.line(&format!("service {name} {{"));
    w.indent();

    let rpcs = children_by_edge(schema, &vertex.id, "field-of");
    for (edge, rpc_vertex) in &rpcs {
        if rpc_vertex.kind != "rpc" {
            continue;
        }
        let rpc_name = edge.name.as_deref().unwrap_or(&rpc_vertex.id);

        // Find input and output type-of edges.
        let type_of_edges: Vec<_> = schema
            .outgoing_edges(&rpc_vertex.id)
            .iter()
            .filter(|e| e.kind == "type-of")
            .collect();

        // The parser stores both input and output as type-of edges.
        // First one is input, second is output (by insertion order).
        let input_type = type_of_edges
            .first()
            .and_then(|e| schema.vertices.get(&e.tgt))
            .map_or("Empty", |v| v.id.rsplit('.').next().unwrap_or(&v.id));

        let output_type = type_of_edges
            .get(1)
            .and_then(|e| schema.vertices.get(&e.tgt))
            .map_or("Empty", |v| v.id.rsplit('.').next().unwrap_or(&v.id));

        w.line(&format!(
            "rpc {rpc_name} ({input_type}) returns ({output_type});"
        ));
    }

    w.dedent();
    w.line("}");
    w.blank();
}

/// Well-formedness rules for Protobuf edges.
fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "field-of".into(),
            src_kinds: vec!["message".into(), "oneof".into(), "service".into()],
            tgt_kinds: vec![
                "field".into(),
                "oneof".into(),
                "rpc".into(),
                "map".into(),
                "string".into(),
                "integer".into(),
                "float".into(),
                "boolean".into(),
            ],
        },
        EdgeRule {
            edge_kind: "type-of".into(),
            src_kinds: vec!["field".into(), "rpc".into(), "map".into()],
            tgt_kinds: vec![
                "message".into(),
                "enum".into(),
                "field".into(),
                "map".into(),
                "string".into(),
                "integer".into(),
                "float".into(),
                "boolean".into(),
            ],
        },
        EdgeRule {
            edge_kind: "variant-of".into(),
            src_kinds: vec!["enum".into()],
            tgt_kinds: vec!["enum-value".into()],
        },
    ]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_creates_valid_definition() {
        let p = protocol();
        assert_eq!(p.name, "protobuf");
        assert_eq!(p.schema_theory, "ThProtobufSchema");
        assert_eq!(p.instance_theory, "ThProtobufInstance");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);

        assert!(registry.contains_key("ThSimpleGraph"));
        assert!(registry.contains_key("ThProtobufSchema"));
        assert!(registry.contains_key("ThProtobufInstance"));
    }

    #[test]
    fn parse_simple_proto() {
        let proto = r"
message Person {
  string name = 1;
  int32 age = 2;
  bool active = 3;
}
";

        let schema = parse_proto(proto);
        assert!(schema.is_ok(), "parse_proto should succeed: {schema:?}");
        let schema = schema.ok();
        let schema = schema.as_ref();

        assert!(schema.is_some_and(|s| s.has_vertex("Person")));
        assert!(schema.is_some_and(|s| s.has_vertex("Person.name")));
        assert!(schema.is_some_and(|s| s.has_vertex("Person.age")));
    }

    #[test]
    fn parse_proto_with_type_of_edges() {
        let proto = r"
message Address {
  string street = 1;
}

message Person {
  string name = 1;
  Address address = 2;
}
";
        let schema = parse_proto(proto).expect("should parse");
        // The field Person.address should have a type-of edge to Address.
        let type_of: Vec<_> = schema
            .outgoing_edges("Person.address")
            .iter()
            .filter(|e| e.kind == "type-of")
            .collect();
        assert_eq!(type_of.len(), 1);
        assert_eq!(type_of[0].tgt, "Address");
    }

    #[test]
    fn parse_proto_with_syntax_and_package() {
        let proto = r#"
syntax = "proto3";
package example;

message Foo {
  string bar = 1;
}
"#;
        let schema = parse_proto(proto).expect("should parse");
        assert!(schema.has_vertex("Foo"));
    }

    #[test]
    fn parse_proto_with_service() {
        let proto = r"
message Request {
  string query = 1;
}

message Response {
  string result = 1;
}

service SearchService {
  rpc Search (Request) returns (Response);
}
";
        let schema = parse_proto(proto).expect("should parse");
        assert!(schema.has_vertex("SearchService"));
        assert!(schema.has_vertex("SearchService.Search"));
        assert_eq!(
            schema.vertices.get("SearchService").unwrap().kind,
            "service"
        );
        assert_eq!(
            schema.vertices.get("SearchService.Search").unwrap().kind,
            "rpc"
        );
    }

    #[test]
    fn emit_proto_roundtrip() {
        let proto = r#"
syntax = "proto3";

message Person {
  string name = 1;
  int32 age = 2;
  bool active = 3;
}
"#;
        let schema1 = parse_proto(proto).expect("first parse should succeed");
        let emitted = emit_proto(&schema1).expect("emit should succeed");
        let schema2 = parse_proto(&emitted).expect("re-parse should succeed");

        assert_eq!(
            schema1.vertex_count(),
            schema2.vertex_count(),
            "vertex counts should match after round-trip"
        );
        assert_eq!(
            schema1.edge_count(),
            schema2.edge_count(),
            "edge counts should match after round-trip"
        );
    }

    #[test]
    fn parse_proto_with_map_field() {
        let proto = r"
message Config {
  map<string, string> labels = 1;
}
";
        let schema = parse_proto(proto).expect("should parse");
        assert!(schema.has_vertex("Config"));
        assert!(schema.has_vertex("Config.labels"));
        assert_eq!(schema.vertices.get("Config.labels").unwrap().kind, "map");
        assert!(schema.has_vertex("Config.labels:key"));
        assert!(schema.has_vertex("Config.labels:value"));
    }
}

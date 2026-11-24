//! The GraphQL type system reader and its canonical writer.
//!
//! [`read_graphql`] reads one GraphQL type system document, and
//! [`read_graphql_bundle`] a type system split over several documents,
//! through the bundled tree-sitter GraphQL grammar
//! ([bkegley/tree-sitter-graphql](https://github.com/bkegley/tree-sitter-graphql))
//! into a schema of the `graphql` protocol
//! ([`panproto_protocols::api::graphql`]), and refuses with a
//! [`GraphQlError`] naming the document, the schema coordinate of the
//! definition at fault and the byte offset of the text at fault every
//! document the reader does not carry exactly. [`write_graphql`] writes such
//! a schema back as canonical GraphQL text.
//!
//! The reader reads the September 2025 edition of the specification
//! ([`EDITION`](panproto_protocols::api::graphql::EDITION)): every type system definition and extension (schema,
//! scalar, object, interface, union, enum and input object types, and
//! directive definitions), with descriptions, default values, applied
//! directives, and the built-in directives `@deprecated`, `@specifiedBy`
//! and `@oneOf`. An executable definition (an operation or a fragment) is
//! refused with [`GraphQlErrorKind::Executable`]. A document the grammar
//! cannot parse is refused at the first place it fails, as a construct the
//! grammar does not read ([`GraphQlErrorKind::NotRead`]) where the failure
//! is one of the grammar's known gaps, and as [`GraphQlErrorKind::Syntax`]
//! otherwise.
//!
//! Every type validation rule of the specification's chapter 3, and the
//! rules of its chapter 5 that a type system document can break (applied
//! directives defined, in valid locations, unique per location, with known,
//! unique and required arguments, and values of the correct type), is
//! checked, and a document breaking one is refused with
//! [`GraphQlErrorKind::Invalid`] naming the [`Rule`]. An interface an
//! object or interface type implements is recorded as an `implements` edge
//! and checked as the specification's `IsValidImplementation` states; no
//! field is inherited through it.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::{self, Write as _};

use panproto_protocols::api::graphql::{
    APPLIED, ARGUMENT, ARGUMENT_EDGE, BUILT_IN, BUILT_IN_DIRECTIVES, BUILT_IN_SCALARS, DEFAULT,
    DEFAULT_DEPRECATION_REASON, DEFAULT_ROOT_NAMES, DEPRECATED, DESCRIPTION, DIRECTIVE, ENUM,
    ENUM_VALUE, FIELD, FIELD_EDGE, IMPLEMENTS, INPUT_FIELD, INPUT_FIELD_EDGE, INPUT_OBJECT,
    INTERFACE, LOCATION, MEMBER, OBJECT, ONE_OF, REPEATABLE, ROOT, SCALAR, SCHEMA, SCHEMA_VERTEX,
    SPECIFIED_BY, TYPE, TYPE_EDGE, TYPE_KINDS, UNION, VALUE_EDGE,
};
use panproto_schema::{Schema, SchemaBuilder};
use tree_sitter::{Node, Parser, Tree};

/// The name the grammar is bundled under.
const GRAMMAR: &str = "graphql";

/// The deepest nesting of list values, object values and list types read.
pub const MAX_NESTING: usize = 256;

thread_local! {
    /// The parser over the bundled grammar, made once per thread.
    static PARSER: RefCell<Option<Parser>> = const { RefCell::new(None) };
}

/// A refusal of the GraphQL reader: what is wrong, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphQlError {
    /// The index of the document at fault in the bundle, `0` for a single
    /// document.
    pub document: usize,
    /// The schema coordinate of the definition at fault (`Type`,
    /// `Type.field`, `Type.field(arg:)`, `@directive`,
    /// `@directive(arg:)`, or `{schema}` for the schema definition), empty
    /// where no definition is read yet.
    pub path: String,
    /// The byte offset of the text at fault in its document.
    pub offset: usize,
    /// What is wrong.
    pub kind: GraphQlErrorKind,
}

impl fmt::Display for GraphQlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = if self.path.is_empty() {
            "the document"
        } else {
            &self.path
        };
        write!(
            f,
            "document {}, {path} (byte {}): {}",
            self.document, self.offset, self.kind
        )
    }
}

impl std::error::Error for GraphQlError {}

/// What a [`GraphQlError`] refuses.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum GraphQlErrorKind {
    /// The document is not UTF-8.
    #[error("the document is not UTF-8")]
    Encoding,
    /// The text is not GraphQL syntax.
    #[error("not GraphQL syntax: {0}")]
    Syntax(&'static str),
    /// A construct of the specification the reader does not read.
    #[error("{0}")]
    NotRead(NotRead),
    /// An executable definition, which a type system document does not
    /// hold.
    #[error("{0} is an executable definition, which the reader does not read")]
    Executable(&'static str),
    /// A type system validation rule the document breaks.
    #[error("{0}")]
    Invalid(Rule),
    /// Values or types nested deeper than [`MAX_NESTING`].
    #[error("nested deeper than {MAX_NESTING} levels")]
    Nesting,
    /// The schema the documents describe cannot be built.
    #[error("the schema cannot be built: {0}")]
    Build(String),
}

/// A construct of the specification the reader does not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum NotRead {
    /// An escape sequence in a string, which the grammar does not parse.
    #[error("an escape sequence in a string, which the bundled grammar does not parse")]
    StringEscape,
    /// A quote just before the closing `"""` of a block string, which the
    /// grammar does not parse.
    #[error(
        "a quote just before the closing \"\"\" of a block string, which the bundled grammar does not parse"
    )]
    BlockStringQuote,
    /// A schema extension with no operation types or with more than one,
    /// which the grammar does not parse.
    #[error(
        "a schema extension with no operation types or more than one, which the bundled grammar does not parse"
    )]
    SchemaExtension,
    /// An extension of a built-in scalar, which the canonical text, which
    /// omits built-in scalars, cannot state.
    #[error("an extension of a built-in scalar, which the reader does not read")]
    BuiltInScalarExtension,
}

/// A type system validation rule of the specification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum Rule {
    /// 2.1.9: a name in a type system starts with `__`.
    #[error("a name must not start with \"__\" (2.1.9)")]
    ReservedName,
    /// 2.9: a string holds a line terminator.
    #[error("a quoted string must not hold a line terminator (2.9.4)")]
    LineTerminatorInString,
    /// 2.9.6: an enum value is `true`, `false` or `null`.
    #[error("an enum value must not be true, false or null (2.9.6)")]
    EnumValueName,
    /// 2.10: a variable in a constant value.
    #[error("a constant value must not hold a variable (2.9)")]
    Variable,
    /// 3.3: two types share a name.
    #[error("every type must have a unique name (3.3)")]
    DuplicateType,
    /// 3.3: a type has the name of a built-in scalar.
    #[error("no type may have the name of a built-in scalar (3.3)")]
    BuiltInTypeName,
    /// 3.3: two directives share a name.
    #[error("every directive must have a unique name (3.3)")]
    DuplicateDirective,
    /// 3.13: a definition of a built-in directive that is not the
    /// specification's.
    #[error("a definition of a built-in directive must be the specification's (3.13)")]
    BuiltInDirectiveRedefined,
    /// 3.3.1: more than one schema definition.
    #[error("a document must include at most one schema definition (3.3.1)")]
    SchemaDefinitions,
    /// 3.3.1: an operation type given twice.
    #[error("each operation type must have one root operation type (3.3.1)")]
    DuplicateOperation,
    /// 3.3.1: no `query` root operation type.
    #[error("the query root operation type must be provided (3.3.1)")]
    QueryRootMissing,
    /// 3.3.1: a root operation type that is not an object type.
    #[error("a root operation type must be an object type (3.3.1)")]
    RootNotObject,
    /// 3.3.1: two operation types with the same root type.
    #[error("the root operation types must all be different types (3.3.1)")]
    RootsNotDistinct,
    /// 3.3.2: a schema extension with no schema definition.
    #[error("a schema extension needs the schema to be defined (3.3.2)")]
    SchemaUndefined,
    /// 2.12: a type reference to a type that is not defined.
    #[error("a named type must exist (2.12)")]
    UnknownType,
    /// 3.4.3: an extension of a type that is not defined.
    #[error("the named type of an extension must already be defined (3.4.3)")]
    ExtensionUndefined,
    /// 3.4.3: an extension of a type of another kind.
    #[error("an extension must be of the kind of the type it extends (3.4.3)")]
    ExtensionKind,
    /// 3.4.3: an extension without directives, interfaces or members.
    #[error("an extension must add directives, interfaces or members (3.4.3)")]
    EmptyExtension,
    /// 3.6, 3.7, 3.10: an object, interface or input object type with no
    /// fields.
    #[error("the type must define one or more fields (3.6, 3.7, 3.10)")]
    NoFields,
    /// 3.6, 3.7, 3.10: two fields with one name.
    #[error("every field must have a unique name in its type (3.6, 3.7, 3.10)")]
    DuplicateField,
    /// 3.6: a field whose type is not an output type.
    #[error("a field must return an output type (3.6)")]
    NotOutputType,
    /// 3.6.1, 3.13: an argument or input field whose type is not an input
    /// type.
    #[error("an argument or input field must accept an input type (3.6.1, 3.10, 3.13)")]
    NotInputType,
    /// 3.6.1, 3.13: two arguments with one name.
    #[error("every argument must have a unique name (3.6, 3.13)")]
    DuplicateArgument,
    /// 3.13.3: `@deprecated` on a required argument or input field.
    #[error("@deprecated must not apply to a required argument or input field (3.13.3)")]
    DeprecatedRequired,
    /// 3.6: an interface named twice in one implements list.
    #[error("the implemented interfaces must be unique (3.6, 3.7)")]
    DuplicateInterface,
    /// 3.6, 3.7: an implemented type that is not an interface.
    #[error("only an interface may be implemented (3.6, 3.7)")]
    ImplementsNonInterface,
    /// 3.7: an interface that implements itself.
    #[error("an interface may not implement itself (3.7)")]
    ImplementsSelf,
    /// 3.6 `IsValidImplementation` 1: an interface of the implemented
    /// interface that the type does not declare.
    #[error("the type must declare the interfaces its interfaces implement (3.6)")]
    TransitiveInterface,
    /// 3.6 `IsValidImplementation` 2: a field of the interface the type
    /// lacks.
    #[error("the type must include every field of its interfaces (3.6)")]
    MissingInterfaceField,
    /// 3.6 `IsValidImplementation` 2.3: an argument of the interface's
    /// field the type's field lacks.
    #[error("the field must include every argument of the interface's field (3.6)")]
    MissingInterfaceArgument,
    /// 3.6 `IsValidImplementation` 2.3.1: an argument of another type than
    /// the interface's.
    #[error("the argument must accept the same type as the interface's (3.6)")]
    ArgumentNotInvariant,
    /// 3.6 `IsValidImplementation` 2.4: a required argument the interface's
    /// field lacks.
    #[error("an argument the interface's field lacks must not be required (3.6)")]
    ExtraArgumentRequired,
    /// 3.6 `IsValidImplementation` 2.5: a field type that is not a subtype
    /// of the interface field's type.
    #[error("the field must return a subtype of the interface field's type (3.6)")]
    FieldNotCovariant,
    /// 3.6 `IsValidImplementation` 2.6: a deprecated field whose interface
    /// field is not deprecated.
    #[error("a deprecated field's interface field must be deprecated too (3.6)")]
    DeprecationNotInherited,
    /// 3.8: a union with no members.
    #[error("a union must include one or more member types (3.8)")]
    NoMembers,
    /// 3.8: a member that is not an object type.
    #[error("the member types of a union must be object types (3.8)")]
    MemberNotObject,
    /// 3.8: a member named twice.
    #[error("the member types of a union must be unique (3.8)")]
    DuplicateMember,
    /// 3.9: an enum with no values.
    #[error("an enum must define one or more values (3.9)")]
    NoValues,
    /// 3.9: two values with one name.
    #[error("the values of an enum must be unique (3.9)")]
    DuplicateValue,
    /// 3.10.1: a non-null field of a `@oneOf` input object.
    #[error("every field of a OneOf Input Object must be nullable (3.10.1)")]
    OneOfNonNull,
    /// 3.10.1: a default value on a field of a `@oneOf` input object.
    #[error("no field of a OneOf Input Object may have a default value (3.10.1)")]
    OneOfDefault,
    /// 3.10.2: `@oneOf` on an input object extension.
    #[error("@oneOf must not be provided by an input object extension (3.10.2)")]
    OneOfOnExtension,
    /// 3.10: an unbroken chain of non-null singular input fields back to
    /// the input object.
    #[error("an input object must not reference itself through non-null singular fields (3.10)")]
    InputCycle,
    /// 3.10 `InputObjectDefaultValueHasCycle`.
    #[error("the default values of an input object must not form a cycle (3.10)")]
    DefaultValueCycle,
    /// 3.13: a directive definition that references itself.
    #[error("a directive definition must not reference itself directly or indirectly (3.13)")]
    DirectiveSelfReference,
    /// 5.7.1: an applied directive that is not defined.
    #[error("an applied directive must be defined (5.7.1)")]
    DirectiveUndefined,
    /// 5.7.2: a directive applied where its definition does not allow it.
    #[error("a directive must be applied in a location its definition lists (5.7.2)")]
    DirectiveLocation,
    /// 5.7.3: a non-repeatable directive applied twice to one definition.
    #[error("a non-repeatable directive must apply at most once to a definition (5.7.3)")]
    DirectiveRepeated,
    /// 5.4.1: an argument the directive does not define.
    #[error("an argument of an applied directive must be defined by the directive (5.4.1)")]
    ArgumentUndefined,
    /// 5.4.2: an argument given twice.
    #[error("an argument must be given at most once (5.4.2)")]
    ArgumentRepeated,
    /// 5.4.2.1: a required argument not given, or given `null`.
    #[error("a required argument must be given and not null (5.4.2.1)")]
    ArgumentMissing,
    /// 5.6.1, 3.6.1: a value that does not coerce to its type.
    #[error("a value must be coercible to its type (5.6.1)")]
    ValueNotCoercible,
}

/// What the reader reads: the schema.
#[derive(Clone, Debug)]
pub struct GraphQlReading {
    /// The schema of the `graphql` protocol.
    pub schema: Schema,
}

type R<T> = Result<T, GraphQlError>;

/// Read `source`, one GraphQL type system document, into a `graphql`
/// schema.
///
/// # Errors
///
/// Returns a [`GraphQlError`] naming the place at fault.
pub fn read_graphql(source: &[u8]) -> Result<GraphQlReading, GraphQlError> {
    read_graphql_bundle(&[source])
}

/// Read the documents of one type system, in order, into one `graphql`
/// schema.
///
/// The definitions of every document make one type system; an extension
/// applies to the definition of its type wherever it is, in the order the
/// extensions are read. The order of the checks, and so which refusal comes
/// first, is the one the book's GraphQL page states: each document's
/// encoding and syntax in document order, then the definitions' names, the
/// extensions, the type references, the root operation types, the rules of
/// each type in the order defined, the directive definitions, the applied
/// directives, and the default values.
///
/// # Errors
///
/// Returns a [`GraphQlError`] naming the place at fault.
pub fn read_graphql_bundle(documents: &[&[u8]]) -> Result<GraphQlReading, GraphQlError> {
    let mut definitions = Definitions::default();
    for (document, bytes) in documents.iter().enumerate() {
        let text = std::str::from_utf8(bytes).map_err(|e| GraphQlError {
            document,
            path: String::new(),
            offset: e.valid_up_to(),
            kind: GraphQlErrorKind::Encoding,
        })?;
        let tree = parse(document, text)?;
        Syntax {
            document,
            src: text,
            out: &mut definitions,
        }
        .document(tree.root_node())?;
    }
    let system = TypeSystem::assemble(definitions)?;
    system.validate()?;
    let schema = system.build()?;
    Ok(GraphQlReading { schema })
}

/// Parse `src` with the bundled GraphQL grammar, refusing a tree with an
/// error at its first one.
fn parse(document: usize, src: &str) -> R<Tree> {
    let unavailable = |reason: &'static str| GraphQlError {
        document,
        path: String::new(),
        offset: 0,
        kind: GraphQlErrorKind::Build(reason.to_owned()),
    };
    let tree = PARSER.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            let grammar = panproto_grammars::grammars()
                .into_iter()
                .find(|g| g.name == GRAMMAR)
                .ok_or_else(|| unavailable("this build did not compile the GraphQL grammar"))?;
            let mut parser = Parser::new();
            parser
                .set_language(&grammar.language)
                .map_err(|_| unavailable("the GraphQL grammar does not load"))?;
            *slot = Some(parser);
        }
        slot.as_mut()
            .and_then(|parser| parser.parse(src, None))
            .ok_or_else(|| unavailable("the parser returned no tree"))
    })?;
    if let Some(node) = first_error(tree.root_node()) {
        return Err(GraphQlError {
            document,
            path: String::new(),
            offset: node.start_byte(),
            kind: classify(src, tree.root_node(), node),
        });
    }
    Ok(tree)
}

/// The first error or missing node of the tree in document order, found
/// without recursion.
fn first_error(root: Node<'_>) -> Option<Node<'_>> {
    if !root.has_error() {
        return None;
    }
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            return Some(node);
        }
        if node.has_error() && cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return None;
            }
        }
    }
}

/// Whether `node` lies in a comment.
fn byte_in_comment(root: Node<'_>, byte: usize) -> bool {
    root.descendant_for_byte_range(byte, byte + 1)
        .is_some_and(|n| n.kind() == "comment")
}

/// What the first error of a document is: a known gap of the grammar, or
/// text that is not GraphQL.
///
/// The grammar parses no escape sequence (a backslash in a quoted string),
/// no quote just before the closing `"""` of a block string, and no schema
/// extension other than `extend schema` with one operation type in braces.
/// An error whose top-level definition starts with `extend schema` is the
/// third; an error preceded, in its top-level definition, by a backslash
/// outside a comment is the first; an error at a quote just after `"""` is
/// the second.
fn classify(src: &str, root: Node<'_>, error: Node<'_>) -> GraphQlErrorKind {
    let bytes = src.as_bytes();
    let mut top = error;
    while let Some(parent) = top.parent() {
        if parent.kind() == "document" || parent.kind() == "source_file" {
            break;
        }
        top = parent;
    }
    let mut leaves = Vec::new();
    let mut cursor = top.walk();
    'walk: loop {
        let node = cursor.node();
        if node.child_count() == 0 && node.kind() != "comment" && node.kind() != "comma" {
            leaves.push(node.kind());
            if leaves.len() == 2 {
                break;
            }
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() || cursor.node() == top {
                break 'walk;
            }
        }
    }
    if leaves == ["extend", "schema"] {
        return GraphQlErrorKind::NotRead(NotRead::SchemaExtension);
    }
    let end = error
        .end_byte()
        .max(error.start_byte() + 1)
        .min(bytes.len());
    if (top.start_byte()..end).any(|i| bytes[i] == b'\\' && !byte_in_comment(root, i)) {
        return GraphQlErrorKind::NotRead(NotRead::StringEscape);
    }
    let start = error.start_byte();
    if start >= 3 && bytes.get(start) == Some(&b'"') && &bytes[start - 3..start] == b"\"\"\"" {
        return GraphQlErrorKind::NotRead(NotRead::BlockStringQuote);
    }
    GraphQlErrorKind::Syntax(if error.is_missing() {
        "a token is missing"
    } else {
        "the grammar cannot parse the text here"
    })
}

// ---------------------------------------------------------------------------
// The definitions of a document, as read from its tree.
// ---------------------------------------------------------------------------

/// A constant input value.
#[derive(Clone, Debug, PartialEq)]
enum Value {
    Int(String),
    Float(String),
    Str(String),
    Bool(bool),
    Null,
    Enum(String),
    List(Vec<Self>),
    Object(Vec<(String, Self)>),
}

/// A type reference.
#[derive(Clone, Debug, PartialEq, Eq)]
enum TypeRef {
    Named(String),
    List(Box<Self>),
    NonNull(Box<Self>),
}

impl TypeRef {
    fn named(&self) -> &str {
        match self {
            Self::Named(name) => name,
            Self::List(inner) | Self::NonNull(inner) => inner.named(),
        }
    }

    const fn is_non_null(&self) -> bool {
        matches!(self, Self::NonNull(_))
    }

    /// The reference as GraphQL text.
    fn text(&self) -> String {
        match self {
            Self::Named(name) => name.clone(),
            Self::List(inner) => format!("[{}]", inner.text()),
            Self::NonNull(inner) => format!("{}!", inner.text()),
        }
    }
}

/// Where a definition is: its document, the byte offset of its text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct At {
    document: usize,
    offset: usize,
}

/// A directive applied to a definition.
#[derive(Clone, Debug)]
struct Applied {
    name: String,
    arguments: Vec<(String, Value, At)>,
    at: At,
}

/// An argument or input field definition.
#[derive(Clone, Debug)]
struct InputValue {
    name: String,
    description: Option<String>,
    ty: TypeRef,
    default: Option<(Value, At)>,
    directives: Vec<Applied>,
    at: At,
}

/// A field definition of an object or interface type.
#[derive(Clone, Debug)]
struct FieldDef {
    name: String,
    description: Option<String>,
    arguments: Vec<InputValue>,
    ty: TypeRef,
    directives: Vec<Applied>,
    at: At,
}

/// An enum value definition.
#[derive(Clone, Debug)]
struct EnumValueDef {
    name: String,
    description: Option<String>,
    directives: Vec<Applied>,
    at: At,
}

/// What a type definition or extension states besides its directives.
#[derive(Clone, Debug, Default)]
struct Body {
    interfaces: Vec<(String, At)>,
    fields: Vec<FieldDef>,
    members: Vec<(String, At)>,
    values: Vec<EnumValueDef>,
    input_fields: Vec<InputValue>,
}

/// A type definition, or an extension of one.
#[derive(Clone, Debug)]
struct TypeDef {
    kind: &'static str,
    name: String,
    description: Option<String>,
    directives: Vec<Applied>,
    body: Body,
    at: At,
}

/// A directive definition.
#[derive(Clone, Debug)]
struct DirectiveDef {
    name: String,
    description: Option<String>,
    arguments: Vec<InputValue>,
    repeatable: bool,
    locations: Vec<String>,
    at: At,
}

/// A schema definition, or an extension of it.
#[derive(Clone, Debug, Default)]
struct SchemaDef {
    description: Option<String>,
    directives: Vec<Applied>,
    roots: Vec<(String, String, At)>,
    at: At,
}

/// Every definition and extension of the documents, in the order read.
#[derive(Debug, Default)]
struct Definitions {
    schemas: Vec<SchemaDef>,
    schema_extensions: Vec<SchemaDef>,
    types: Vec<TypeDef>,
    extensions: Vec<TypeDef>,
    directives: Vec<DirectiveDef>,
}

/// The walk of one document's tree into [`Definitions`].
struct Syntax<'s, 'o> {
    document: usize,
    src: &'s str,
    out: &'o mut Definitions,
}

/// The children of `node` in order, without comments and commas.
fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|c| c.kind() != "comment" && c.kind() != "comma")
        .collect()
}

/// The first child of `node` of kind `kind`.
fn child<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    children(node).into_iter().find(|c| c.kind() == kind)
}

/// The children of a left-recursive list node (`implements_interfaces`,
/// `union_member_types`, `directive_locations`) of kind `kind`, in order.
fn flatten<'t>(node: Node<'t>, kind: &str) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    let mut current = Some(node);
    let mut reversed = Vec::new();
    while let Some(list) = current {
        current = None;
        for c in children(list).into_iter().rev() {
            if c.kind() == list.kind() {
                current = Some(c);
            } else if c.kind() == kind {
                reversed.push(c);
            }
        }
    }
    out.extend(reversed.into_iter().rev());
    out
}

impl Syntax<'_, '_> {
    fn text(&self, node: Node<'_>) -> String {
        self.src[node.start_byte()..node.end_byte()].to_owned()
    }

    fn at(&self, node: Node<'_>) -> At {
        At {
            document: self.document,
            offset: node.start_byte(),
        }
    }

    fn fail(&self, path: &str, node: Node<'_>, kind: GraphQlErrorKind) -> GraphQlError {
        GraphQlError {
            document: self.document,
            path: path.to_owned(),
            offset: node.start_byte(),
            kind,
        }
    }

    fn invalid(&self, path: &str, node: Node<'_>, rule: Rule) -> GraphQlError {
        self.fail(path, node, GraphQlErrorKind::Invalid(rule))
    }

    fn document(&mut self, root: Node<'_>) -> R<()> {
        let Some(document) = child(root, "document") else {
            return Err(self.fail(
                "",
                root,
                GraphQlErrorKind::Syntax("a document must hold at least one definition"),
            ));
        };
        for definition in children(document) {
            let Some(inner) = children(definition).into_iter().next() else {
                continue;
            };
            match inner.kind() {
                "executable_definition" => {
                    let what = if children(inner)
                        .first()
                        .is_some_and(|n| n.kind() == "fragment_definition")
                    {
                        "a fragment"
                    } else {
                        "an operation"
                    };
                    return Err(self.fail("", inner, GraphQlErrorKind::Executable(what)));
                }
                "type_system_definition" => {
                    let Some(def) = children(inner).into_iter().next() else {
                        continue;
                    };
                    match def.kind() {
                        "schema_definition" => {
                            let schema = self.schema(def, false)?;
                            self.out.schemas.push(schema);
                        }
                        "directive_definition" => {
                            let directive = self.directive_definition(def)?;
                            self.out.directives.push(directive);
                        }
                        _ => {
                            let Some(ty) = children(def).into_iter().next() else {
                                continue;
                            };
                            let ty = self.type_definition(ty, false)?;
                            self.out.types.push(ty);
                        }
                    }
                }
                _ => {
                    let Some(ext) = children(inner).into_iter().next() else {
                        continue;
                    };
                    if ext.kind() == "schema_extension" {
                        let schema = self.schema(ext, true)?;
                        self.out.schema_extensions.push(schema);
                    } else if let Some(ty) = children(ext).into_iter().next() {
                        let ty = self.type_definition(ty, true)?;
                        self.out.extensions.push(ty);
                    }
                }
            }
        }
        Ok(())
    }

    fn description(&self, path: &str, node: Node<'_>) -> R<Option<String>> {
        child(node, "description")
            .and_then(|d| child(d, "string_value"))
            .map(|s| self.string(path, s))
            .transpose()
    }

    /// The value of a `string_value` node.
    fn string(&self, path: &str, node: Node<'_>) -> R<String> {
        let raw = &self.src[node.start_byte()..node.end_byte()];
        if let Some(block) = raw
            .strip_prefix("\"\"\"")
            .and_then(|r| r.strip_suffix("\"\"\""))
        {
            return Ok(block_string_value(block));
        }
        let inner = raw
            .strip_prefix('"')
            .and_then(|r| r.strip_suffix('"'))
            .unwrap_or_default();
        if inner.contains(['\n', '\r']) {
            return Err(self.invalid(path, node, Rule::LineTerminatorInString));
        }
        Ok(inner.to_owned())
    }

    fn name(&self, node: Node<'_>) -> String {
        child(node, "name").map_or_else(String::new, |n| self.text(n))
    }

    fn schema(&self, node: Node<'_>, extension: bool) -> R<SchemaDef> {
        let path = SCHEMA_VERTEX;
        let description = if extension {
            None
        } else {
            self.description(path, node)?
        };
        let directives = self.directives(path, node)?;
        let mut roots = Vec::new();
        for root in children(node)
            .into_iter()
            .filter(|c| c.kind() == "root_operation_type_definition")
        {
            let operation =
                child(root, "operation_type").map_or_else(String::new, |o| self.text(o));
            let target = child(root, "named_type").map_or_else(String::new, |t| self.name(t));
            roots.push((operation, target, self.at(root)));
        }
        Ok(SchemaDef {
            description,
            directives,
            roots,
            at: self.at(node),
        })
    }

    fn reserved(&self, path: &str, node: Node<'_>, name: &str) -> R<()> {
        if name.starts_with("__") {
            Err(self.invalid(path, node, Rule::ReservedName))
        } else {
            Ok(())
        }
    }

    fn type_definition(&self, node: Node<'_>, extension: bool) -> R<TypeDef> {
        let kind = match node.kind() {
            "scalar_type_definition" | "scalar_type_extension" => SCALAR,
            "object_type_definition" | "object_type_extension" => OBJECT,
            "interface_type_definition" | "interface_type_extension" => INTERFACE,
            "union_type_definition" | "union_type_extension" => UNION,
            "enum_type_definition" | "enum_type_extension" => ENUM,
            _ => INPUT_OBJECT,
        };
        let name = self.name(node);
        let path = name.as_str();
        self.reserved(path, node, path)?;
        let description = if extension {
            None
        } else {
            self.description(path, node)?
        };
        let directives = self.directives(path, node)?;
        let mut body = Body::default();
        if let Some(list) = child(node, "implements_interfaces") {
            for named in flatten(list, "named_type") {
                body.interfaces.push((self.name(named), self.at(named)));
            }
        }
        if let Some(fields) = child(node, "fields_definition") {
            for field in children(fields)
                .into_iter()
                .filter(|c| c.kind() == "field_definition")
            {
                body.fields.push(self.field(path, field)?);
            }
        }
        if let Some(list) = child(node, "union_member_types") {
            for named in flatten(list, "named_type") {
                body.members.push((self.name(named), self.at(named)));
            }
        }
        if let Some(values) = child(node, "enum_values_definition") {
            for value in children(values)
                .into_iter()
                .filter(|c| c.kind() == "enum_value_definition")
            {
                body.values.push(self.enum_value(path, value)?);
            }
        }
        let blocks: Vec<Node<'_>> = children(node)
            .into_iter()
            .filter(|c| c.kind() == "input_fields_definition")
            .collect();
        if blocks.len() > 1 {
            return Err(self.fail(
                path,
                blocks[1],
                GraphQlErrorKind::Syntax("an input object extension holds one field block"),
            ));
        }
        if let Some(fields) = blocks.first() {
            for field in children(*fields)
                .into_iter()
                .filter(|c| c.kind() == "input_value_definition")
            {
                body.input_fields
                    .push(self.input_value(&|n| format!("{path}.{n}"), field)?);
            }
        }
        if extension
            && directives.is_empty()
            && body.interfaces.is_empty()
            && body.fields.is_empty()
            && body.members.is_empty()
            && body.values.is_empty()
            && body.input_fields.is_empty()
        {
            return Err(self.invalid(path, node, Rule::EmptyExtension));
        }
        Ok(TypeDef {
            kind,
            name,
            description,
            directives,
            body,
            at: self.at(node),
        })
    }

    fn field(&self, owner: &str, node: Node<'_>) -> R<FieldDef> {
        let name = self.name(node);
        let path = format!("{owner}.{name}");
        self.reserved(&path, node, &name)?;
        let description = self.description(&path, node)?;
        let mut arguments = Vec::new();
        if let Some(list) = child(node, "arguments_definition") {
            for argument in children(list)
                .into_iter()
                .filter(|c| c.kind() == "input_value_definition")
            {
                arguments.push(self.input_value(&|n| format!("{path}({n}:)"), argument)?);
            }
        }
        let ty = self.type_ref(&path, node, 0)?;
        let directives = self.directives(&path, node)?;
        Ok(FieldDef {
            name,
            description,
            arguments,
            ty,
            directives,
            at: self.at(node),
        })
    }

    fn input_value(&self, coordinate: &dyn Fn(&str) -> String, node: Node<'_>) -> R<InputValue> {
        let name = self.name(node);
        let path = coordinate(&name);
        self.reserved(&path, node, &name)?;
        let description = self.description(&path, node)?;
        let ty = self.type_ref(&path, node, 0)?;
        let default = child(node, "default_value")
            .and_then(|d| child(d, "value"))
            .map(|v| Ok::<_, GraphQlError>((self.value(&path, v, 0)?, self.at(v))))
            .transpose()?;
        let directives = self.directives(&path, node)?;
        Ok(InputValue {
            name,
            description,
            ty,
            default,
            directives,
            at: self.at(node),
        })
    }

    fn enum_value(&self, owner: &str, node: Node<'_>) -> R<EnumValueDef> {
        let name = child(node, "enum_value").map_or_else(String::new, |v| self.name(v));
        let path = format!("{owner}.{name}");
        if matches!(name.as_str(), "true" | "false" | "null") {
            return Err(self.invalid(&path, node, Rule::EnumValueName));
        }
        self.reserved(&path, node, &name)?;
        Ok(EnumValueDef {
            description: self.description(&path, node)?,
            directives: self.directives(&path, node)?,
            name,
            at: self.at(node),
        })
    }

    fn directive_definition(&self, node: Node<'_>) -> R<DirectiveDef> {
        let name = self.name(node);
        let path = format!("@{name}");
        self.reserved(&path, node, &name)?;
        let description = self.description(&path, node)?;
        let mut arguments = Vec::new();
        if let Some(list) = child(node, "arguments_definition") {
            for argument in children(list)
                .into_iter()
                .filter(|c| c.kind() == "input_value_definition")
            {
                arguments.push(self.input_value(&|n| format!("{path}({n}:)"), argument)?);
            }
        }
        let repeatable = children(node).iter().any(|c| c.kind() == "repeatable");
        let locations = child(node, "directive_locations")
            .map(|l| {
                flatten(l, "directive_location")
                    .into_iter()
                    .map(|loc| self.text(loc).trim().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        Ok(DirectiveDef {
            name,
            description,
            arguments,
            repeatable,
            locations,
            at: self.at(node),
        })
    }

    /// The type reference child of `node`.
    fn type_ref(&self, path: &str, node: Node<'_>, depth: usize) -> R<TypeRef> {
        let Some(ty) = child(node, "type") else {
            return Err(self.fail(path, node, GraphQlErrorKind::Syntax("a type is missing")));
        };
        self.type_node(path, ty, depth)
    }

    fn type_node(&self, path: &str, node: Node<'_>, depth: usize) -> R<TypeRef> {
        if depth > MAX_NESTING {
            return Err(self.fail(path, node, GraphQlErrorKind::Nesting));
        }
        let Some(inner) = children(node).into_iter().find(Node::is_named) else {
            return Err(self.fail(path, node, GraphQlErrorKind::Syntax("a type is missing")));
        };
        match inner.kind() {
            "named_type" => Ok(TypeRef::Named(self.name(inner))),
            "list_type" => Ok(TypeRef::List(Box::new(self.type_ref(
                path,
                inner,
                depth + 1,
            )?))),
            "type" => self.type_node(path, inner, depth + 1),
            _ => {
                let wrapped = children(inner)
                    .into_iter()
                    .find(Node::is_named)
                    .ok_or_else(|| {
                        self.fail(path, inner, GraphQlErrorKind::Syntax("a type is missing"))
                    })?;
                let nullable = if wrapped.kind() == "named_type" {
                    TypeRef::Named(self.name(wrapped))
                } else {
                    TypeRef::List(Box::new(self.type_ref(path, wrapped, depth + 1)?))
                };
                Ok(TypeRef::NonNull(Box::new(nullable)))
            }
        }
    }

    fn directives(&self, path: &str, node: Node<'_>) -> R<Vec<Applied>> {
        let mut out = Vec::new();
        if let Some(list) = child(node, "directives") {
            for directive in children(list)
                .into_iter()
                .filter(|c| c.kind() == "directive")
            {
                let mut arguments = Vec::new();
                if let Some(args) = child(directive, "arguments") {
                    for argument in children(args)
                        .into_iter()
                        .filter(|c| c.kind() == "argument")
                    {
                        let Some(value) = child(argument, "value") else {
                            continue;
                        };
                        arguments.push((
                            self.name(argument),
                            self.value(path, value, 0)?,
                            self.at(argument),
                        ));
                    }
                }
                out.push(Applied {
                    name: self.name(directive),
                    arguments,
                    at: self.at(directive),
                });
            }
        }
        Ok(out)
    }

    /// A constant value: a `value` node.
    fn value(&self, path: &str, node: Node<'_>, depth: usize) -> R<Value> {
        if depth > MAX_NESTING {
            return Err(self.fail(path, node, GraphQlErrorKind::Nesting));
        }
        let Some(inner) = children(node).into_iter().find(Node::is_named) else {
            return Err(self.fail(path, node, GraphQlErrorKind::Syntax("a value is missing")));
        };
        Ok(match inner.kind() {
            "variable" => return Err(self.invalid(path, inner, Rule::Variable)),
            "int_value" => Value::Int(self.text(inner)),
            "float_value" => Value::Float(self.text(inner)),
            "string_value" => Value::Str(self.string(path, inner)?),
            "boolean_value" => Value::Bool(self.text(inner) == "true"),
            "null_value" => Value::Null,
            "enum_value" => {
                let name = self.name(inner);
                if matches!(name.as_str(), "true" | "false" | "null") {
                    return Err(self.invalid(path, inner, Rule::EnumValueName));
                }
                Value::Enum(name)
            }
            "list_value" => {
                let mut items = Vec::new();
                for item in children(inner).into_iter().filter(|c| c.kind() == "value") {
                    items.push(self.value(path, item, depth + 1)?);
                }
                Value::List(items)
            }
            _ => {
                let mut fields = Vec::new();
                for field in children(inner)
                    .into_iter()
                    .filter(|c| c.kind() == "object_field")
                {
                    let Some(value) = child(field, "value") else {
                        continue;
                    };
                    fields.push((self.name(field), self.value(path, value, depth + 1)?));
                }
                Value::Object(fields)
            }
        })
    }
}

/// The specification's `BlockStringValue(rawValue)`.
fn block_string_value(raw: &str) -> String {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    let indent_of = |line: &str| line.len() - line.trim_start_matches([' ', '\t']).len();
    let common = lines
        .iter()
        .skip(1)
        .filter(|line| indent_of(line) < line.len())
        .map(|line| indent_of(line))
        .min();
    let mut owned: Vec<String> = Vec::with_capacity(lines.len());
    for (i, line) in lines.into_iter().enumerate() {
        match common {
            Some(n) if i > 0 => owned.push(line.get(n..).unwrap_or_default().to_owned()),
            _ => owned.push(line.to_owned()),
        }
    }
    let blank = |line: &String| line.chars().all(|c| c == ' ' || c == '\t');
    while owned.first().is_some_and(blank) {
        owned.remove(0);
    }
    while owned.last().is_some_and(blank) {
        owned.pop();
    }
    owned.join("\n")
}

/// The GraphQL text of `value`: a quoted string where the grammar reads
/// one (no quote, backslash or line terminator), else a block string
/// whose value is `value`, or `None` where neither states it.
#[must_use]
pub fn string_text(value: &str) -> Option<String> {
    if !value.contains(['"', '\\', '\n', '\r']) {
        return Some(format!("\"{value}\""));
    }
    [
        format!("\n{value}\n"),
        value.to_owned(),
        format!("{value}\n"),
    ]
    .into_iter()
    .find(|raw| {
        !raw.contains("\"\"\"")
            && !raw.contains('\r')
            && !raw.starts_with('"')
            && !raw.ends_with('"')
            && block_string_value(raw) == value
    })
    .map(|raw| format!("\"\"\"{raw}\"\"\""))
}

/// The canonical GraphQL text of a value.
fn value_text(value: &Value) -> Option<String> {
    Some(match value {
        Value::Int(text) | Value::Float(text) | Value::Enum(text) => text.clone(),
        Value::Str(text) => string_text(text)?,
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_owned(),
        Value::List(items) => {
            let items: Option<Vec<String>> = items.iter().map(value_text).collect();
            format!("[{}]", items?.join(", "))
        }
        Value::Object(fields) => {
            let fields: Option<Vec<String>> = fields
                .iter()
                .map(|(name, value)| Some(format!("{name}: {}", value_text(value)?)))
                .collect();
            format!("{{{}}}", fields?.join(", "))
        }
    })
}

/// The canonical GraphQL text of an applied directive.
fn applied_text(applied: &Applied) -> Option<String> {
    if applied.arguments.is_empty() {
        return Some(format!("@{}", applied.name));
    }
    let arguments: Option<Vec<String>> = applied
        .arguments
        .iter()
        .map(|(name, value, _)| Some(format!("{name}: {}", value_text(value)?)))
        .collect();
    Some(format!("@{}({})", applied.name, arguments?.join(", ")))
}

// ---------------------------------------------------------------------------
// The type system the definitions make.
// ---------------------------------------------------------------------------

/// A definition's place in the specification's directive locations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Location {
    Schema,
    Scalar,
    Object,
    FieldDefinition,
    ArgumentDefinition,
    Interface,
    Union,
    Enum,
    EnumValue,
    InputObject,
    InputFieldDefinition,
}

impl Location {
    const fn name(self) -> &'static str {
        match self {
            Self::Schema => "SCHEMA",
            Self::Scalar => "SCALAR",
            Self::Object => "OBJECT",
            Self::FieldDefinition => "FIELD_DEFINITION",
            Self::ArgumentDefinition => "ARGUMENT_DEFINITION",
            Self::Interface => "INTERFACE",
            Self::Union => "UNION",
            Self::Enum => "ENUM",
            Self::EnumValue => "ENUM_VALUE",
            Self::InputObject => "INPUT_OBJECT",
            Self::InputFieldDefinition => "INPUT_FIELD_DEFINITION",
        }
    }

    fn of_kind(kind: &str) -> Self {
        match kind {
            SCALAR => Self::Scalar,
            OBJECT => Self::Object,
            INTERFACE => Self::Interface,
            UNION => Self::Union,
            ENUM => Self::Enum,
            _ => Self::InputObject,
        }
    }
}

/// The definitions of the specification's built-in directives.
fn built_in_directives() -> Vec<DirectiveDef> {
    let at = At::default();
    let argument = |name: &str, ty: TypeRef, default: Option<Value>| InputValue {
        name: name.to_owned(),
        description: None,
        ty,
        default: default.map(|v| (v, at)),
        directives: Vec::new(),
        at,
    };
    let non_null = |name: &str| TypeRef::NonNull(Box::new(TypeRef::Named(name.to_owned())));
    let directive = |name: &str, arguments: Vec<InputValue>, locations: &[&str]| DirectiveDef {
        name: name.to_owned(),
        description: None,
        arguments,
        repeatable: false,
        locations: locations.iter().map(|l| (*l).to_owned()).collect(),
        at,
    };
    let conditional = &["FIELD", "FRAGMENT_SPREAD", "INLINE_FRAGMENT"];
    vec![
        directive(
            "skip",
            vec![argument("if", non_null("Boolean"), None)],
            conditional,
        ),
        directive(
            "include",
            vec![argument("if", non_null("Boolean"), None)],
            conditional,
        ),
        directive(
            "deprecated",
            vec![argument(
                "reason",
                non_null("String"),
                Some(Value::Str(DEFAULT_DEPRECATION_REASON.to_owned())),
            )],
            &[
                "FIELD_DEFINITION",
                "ARGUMENT_DEFINITION",
                "INPUT_FIELD_DEFINITION",
                "ENUM_VALUE",
            ],
        ),
        directive(
            "specifiedBy",
            vec![argument("url", non_null("String"), None)],
            &["SCALAR"],
        ),
        directive("oneOf", Vec::new(), &["INPUT_OBJECT"]),
    ]
}

/// Whether a written definition of a built-in directive is the
/// specification's: the same arguments, types, defaults, repeatability and
/// locations, with no descriptions and no applied directives.
fn same_directive(written: &DirectiveDef, spec: &DirectiveDef) -> bool {
    written.description.is_none()
        && written.repeatable == spec.repeatable
        && written.locations == spec.locations
        && written.arguments.len() == spec.arguments.len()
        && written.arguments.iter().zip(&spec.arguments).all(|(w, s)| {
            w.name == s.name
                && w.ty == s.ty
                && w.description.is_none()
                && w.directives.is_empty()
                && w.default.as_ref().map(|d| &d.0) == s.default.as_ref().map(|d| &d.0)
        })
}

/// The type system: every type with its extensions merged, the directive
/// definitions, the schema.
struct TypeSystem {
    types: Vec<TypeDef>,
    index: HashMap<String, usize>,
    directives: Vec<DirectiveDef>,
    built_ins: Vec<DirectiveDef>,
    schema: SchemaDef,
}

fn error(path: impl Into<String>, at: At, rule: Rule) -> GraphQlError {
    GraphQlError {
        document: at.document,
        path: path.into(),
        offset: at.offset,
        kind: GraphQlErrorKind::Invalid(rule),
    }
}

/// Whether `value` is deprecated: has `@deprecated` applied.
fn deprecated(directives: &[Applied]) -> bool {
    directives.iter().any(|d| d.name == "deprecated")
}

/// Whether an argument or input field is required: non-null with no
/// default value.
const fn required(value: &InputValue) -> bool {
    value.ty.is_non_null() && value.default.is_none()
}

impl TypeSystem {
    /// Check the definitions' names, merge the extensions into their types,
    /// and settle the schema.
    fn assemble(definitions: Definitions) -> R<Self> {
        let Definitions {
            schemas,
            schema_extensions,
            types,
            extensions,
            directives,
        } = definitions;
        let mut index = HashMap::new();
        let mut merged: Vec<TypeDef> = Vec::with_capacity(types.len());
        for ty in types {
            if BUILT_IN_SCALARS.contains(&ty.name.as_str()) {
                return Err(error(ty.name.clone(), ty.at, Rule::BuiltInTypeName));
            }
            if index.contains_key(&ty.name) {
                return Err(error(ty.name.clone(), ty.at, Rule::DuplicateType));
            }
            index.insert(ty.name.clone(), merged.len());
            merged.push(ty);
        }
        let built_ins = built_in_directives();
        let mut defined: Vec<DirectiveDef> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for directive in directives {
            let path = format!("@{}", directive.name);
            if !seen.insert(directive.name.clone()) {
                return Err(error(path, directive.at, Rule::DuplicateDirective));
            }
            if let Some(spec) = built_ins.iter().find(|b| b.name == directive.name) {
                if !same_directive(&directive, spec) {
                    return Err(error(path, directive.at, Rule::BuiltInDirectiveRedefined));
                }
                continue;
            }
            defined.push(directive);
        }
        let mut schemas = schemas.into_iter();
        let schema = schemas.next();
        if let Some(second) = schemas.next() {
            return Err(error(SCHEMA_VERTEX, second.at, Rule::SchemaDefinitions));
        }
        for extension in extensions {
            let path = extension.name.clone();
            let Some(&i) = index.get(&extension.name) else {
                if extension.kind == SCALAR && BUILT_IN_SCALARS.contains(&path.as_str()) {
                    return Err(GraphQlError {
                        document: extension.at.document,
                        path,
                        offset: extension.at.offset,
                        kind: GraphQlErrorKind::NotRead(NotRead::BuiltInScalarExtension),
                    });
                }
                return Err(error(path, extension.at, Rule::ExtensionUndefined));
            };
            let target = &mut merged[i];
            if target.kind != extension.kind {
                return Err(error(path, extension.at, Rule::ExtensionKind));
            }
            merge(target, extension)?;
        }
        let explicit = schema.is_some();
        let mut schema = schema.unwrap_or_else(|| SchemaDef {
            roots: DEFAULT_ROOT_NAMES
                .iter()
                .filter_map(|(operation, name)| {
                    index
                        .get(*name)
                        .map(|&i| ((*operation).to_owned(), (*name).to_owned(), merged[i].at))
                })
                .collect(),
            ..SchemaDef::default()
        });
        for extension in schema_extensions {
            if !explicit {
                return Err(error(SCHEMA_VERTEX, extension.at, Rule::SchemaUndefined));
            }
            schema.directives.extend(extension.directives);
            schema.roots.extend(extension.roots);
        }
        Ok(Self {
            types: merged,
            index,
            directives: defined,
            built_ins,
            schema,
        })
    }

    fn ty(&self, name: &str) -> Option<&TypeDef> {
        self.index.get(name).map(|&i| &self.types[i])
    }

    fn kind_of(&self, name: &str) -> Option<&'static str> {
        if BUILT_IN_SCALARS.contains(&name) {
            return Some(SCALAR);
        }
        self.ty(name).map(|t| t.kind)
    }

    fn directive(&self, name: &str) -> Option<&DirectiveDef> {
        self.directives
            .iter()
            .chain(&self.built_ins)
            .find(|d| d.name == name)
    }

    fn is_input(&self, ty: &TypeRef) -> bool {
        matches!(self.kind_of(ty.named()), Some(SCALAR | ENUM | INPUT_OBJECT))
    }

    fn is_output(&self, ty: &TypeRef) -> bool {
        matches!(
            self.kind_of(ty.named()),
            Some(SCALAR | OBJECT | INTERFACE | UNION | ENUM)
        )
    }

    /// Every check after assembly, in the order the book's page states.
    fn validate(&self) -> R<()> {
        self.check_references()?;
        self.check_roots()?;
        for ty in &self.types {
            self.check_type(ty)?;
        }
        for directive in &self.directives {
            self.check_directive_definition(directive)?;
        }
        self.check_applied()?;
        self.check_defaults()?;
        Ok(())
    }

    fn check_references(&self) -> R<()> {
        let known = |name: &str| self.kind_of(name).is_some();
        for (_, target, at) in &self.schema.roots {
            if !known(target) {
                return Err(error(SCHEMA_VERTEX, *at, Rule::UnknownType));
            }
        }
        for ty in &self.types {
            for (name, at) in ty.body.interfaces.iter().chain(&ty.body.members) {
                if !known(name) {
                    return Err(error(ty.name.clone(), *at, Rule::UnknownType));
                }
            }
            for field in &ty.body.fields {
                let path = format!("{}.{}", ty.name, field.name);
                if !known(field.ty.named()) {
                    return Err(error(path, field.at, Rule::UnknownType));
                }
                for argument in &field.arguments {
                    if !known(argument.ty.named()) {
                        return Err(error(
                            format!("{path}({}:)", argument.name),
                            argument.at,
                            Rule::UnknownType,
                        ));
                    }
                }
            }
            for field in &ty.body.input_fields {
                if !known(field.ty.named()) {
                    return Err(error(
                        format!("{}.{}", ty.name, field.name),
                        field.at,
                        Rule::UnknownType,
                    ));
                }
            }
        }
        for directive in &self.directives {
            for argument in &directive.arguments {
                if !known(argument.ty.named()) {
                    return Err(error(
                        format!("@{}({}:)", directive.name, argument.name),
                        argument.at,
                        Rule::UnknownType,
                    ));
                }
            }
        }
        Ok(())
    }

    fn check_roots(&self) -> R<()> {
        let mut operations: HashSet<&str> = HashSet::new();
        let mut targets: HashSet<&str> = HashSet::new();
        for (operation, target, at) in &self.schema.roots {
            if !operations.insert(operation) {
                return Err(error(SCHEMA_VERTEX, *at, Rule::DuplicateOperation));
            }
            if self.kind_of(target) != Some(OBJECT) {
                return Err(error(SCHEMA_VERTEX, *at, Rule::RootNotObject));
            }
            if !targets.insert(target) {
                return Err(error(SCHEMA_VERTEX, *at, Rule::RootsNotDistinct));
            }
        }
        if !operations.contains("query") {
            return Err(error(SCHEMA_VERTEX, self.schema.at, Rule::QueryRootMissing));
        }
        Ok(())
    }

    /// The rules of an argument or input field definition.
    fn check_input_value(&self, path: &str, value: &InputValue) -> R<()> {
        if !self.is_input(&value.ty) {
            return Err(error(path, value.at, Rule::NotInputType));
        }
        if required(value) && deprecated(&value.directives) {
            return Err(error(path, value.at, Rule::DeprecatedRequired));
        }
        Ok(())
    }

    fn check_arguments(&self, owner: &str, arguments: &[InputValue]) -> R<()> {
        let mut names = HashSet::new();
        for argument in arguments {
            let path = format!("{owner}({}:)", argument.name);
            if !names.insert(argument.name.as_str()) {
                return Err(error(path, argument.at, Rule::DuplicateArgument));
            }
            self.check_input_value(&path, argument)?;
        }
        Ok(())
    }

    fn check_type(&self, ty: &TypeDef) -> R<()> {
        let name = ty.name.as_str();
        match ty.kind {
            OBJECT | INTERFACE => {
                if ty.body.fields.is_empty() {
                    return Err(error(name, ty.at, Rule::NoFields));
                }
                let mut names = HashSet::new();
                for field in &ty.body.fields {
                    let path = format!("{name}.{}", field.name);
                    if !names.insert(field.name.as_str()) {
                        return Err(error(path, field.at, Rule::DuplicateField));
                    }
                    if !self.is_output(&field.ty) {
                        return Err(error(path, field.at, Rule::NotOutputType));
                    }
                    self.check_arguments(&path, &field.arguments)?;
                }
                let mut seen = HashSet::new();
                for (interface, at) in &ty.body.interfaces {
                    if !seen.insert(interface.as_str()) {
                        return Err(error(name, *at, Rule::DuplicateInterface));
                    }
                    if interface == name && ty.kind == INTERFACE {
                        return Err(error(name, *at, Rule::ImplementsSelf));
                    }
                    let Some(implemented) = self.ty(interface).filter(|t| t.kind == INTERFACE)
                    else {
                        return Err(error(name, *at, Rule::ImplementsNonInterface));
                    };
                    self.valid_implementation(ty, implemented, *at)?;
                }
            }
            UNION => {
                if ty.body.members.is_empty() {
                    return Err(error(name, ty.at, Rule::NoMembers));
                }
                let mut seen = HashSet::new();
                for (member, at) in &ty.body.members {
                    if self.kind_of(member) != Some(OBJECT) {
                        return Err(error(name, *at, Rule::MemberNotObject));
                    }
                    if !seen.insert(member.as_str()) {
                        return Err(error(name, *at, Rule::DuplicateMember));
                    }
                }
            }
            ENUM => {
                if ty.body.values.is_empty() {
                    return Err(error(name, ty.at, Rule::NoValues));
                }
                let mut seen = HashSet::new();
                for value in &ty.body.values {
                    if !seen.insert(value.name.as_str()) {
                        return Err(error(
                            format!("{name}.{}", value.name),
                            value.at,
                            Rule::DuplicateValue,
                        ));
                    }
                }
            }
            INPUT_OBJECT => {
                if ty.body.input_fields.is_empty() {
                    return Err(error(name, ty.at, Rule::NoFields));
                }
                let one_of = ty.directives.iter().any(|d| d.name == "oneOf");
                let mut names = HashSet::new();
                for field in &ty.body.input_fields {
                    let path = format!("{name}.{}", field.name);
                    if !names.insert(field.name.as_str()) {
                        return Err(error(path, field.at, Rule::DuplicateField));
                    }
                    self.check_input_value(&path, field)?;
                    if one_of && field.ty.is_non_null() {
                        return Err(error(path, field.at, Rule::OneOfNonNull));
                    }
                    if one_of && field.default.is_some() {
                        return Err(error(path, field.at, Rule::OneOfDefault));
                    }
                }
                self.check_input_cycle(ty)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// `IsValidImplementation(type, implementedType)`.
    fn valid_implementation(&self, ty: &TypeDef, implemented: &TypeDef, at: At) -> R<()> {
        let name = ty.name.as_str();
        for (interface, _) in &implemented.body.interfaces {
            if !ty.body.interfaces.iter().any(|(i, _)| i == interface) {
                return Err(error(name, at, Rule::TransitiveInterface));
            }
        }
        for implemented_field in &implemented.body.fields {
            let Some(field) = ty
                .body
                .fields
                .iter()
                .find(|f| f.name == implemented_field.name)
            else {
                return Err(error(name, at, Rule::MissingInterfaceField));
            };
            let path = format!("{name}.{}", field.name);
            for implemented_argument in &implemented_field.arguments {
                let Some(argument) = field
                    .arguments
                    .iter()
                    .find(|a| a.name == implemented_argument.name)
                else {
                    return Err(error(path, field.at, Rule::MissingInterfaceArgument));
                };
                if argument.ty != implemented_argument.ty {
                    return Err(error(
                        format!("{path}({}:)", argument.name),
                        argument.at,
                        Rule::ArgumentNotInvariant,
                    ));
                }
            }
            for argument in &field.arguments {
                let declared = implemented_field
                    .arguments
                    .iter()
                    .any(|a| a.name == argument.name);
                if !declared && required(argument) {
                    return Err(error(
                        format!("{path}({}:)", argument.name),
                        argument.at,
                        Rule::ExtraArgumentRequired,
                    ));
                }
            }
            if !self.valid_field_type(&field.ty, &implemented_field.ty) {
                return Err(error(path, field.at, Rule::FieldNotCovariant));
            }
            if deprecated(&field.directives) && !deprecated(&implemented_field.directives) {
                return Err(error(path, field.at, Rule::DeprecationNotInherited));
            }
        }
        Ok(())
    }

    /// `IsValidImplementationFieldType(fieldType, implementedFieldType)`.
    fn valid_field_type(&self, field: &TypeRef, implemented: &TypeRef) -> bool {
        match (field, implemented) {
            (TypeRef::NonNull(inner), TypeRef::NonNull(other)) => {
                self.valid_field_type(inner, other)
            }
            (TypeRef::NonNull(inner), other) => self.valid_field_type(inner, other),
            (TypeRef::List(inner), TypeRef::List(other)) => self.valid_field_type(inner, other),
            (TypeRef::Named(sub), TypeRef::Named(sup)) => self.is_sub_type(sub, sup),
            _ => false,
        }
    }

    /// `IsSubType(possibleSubType, superType)` for named types.
    fn is_sub_type(&self, sub: &str, sup: &str) -> bool {
        if sub == sup {
            return true;
        }
        let (Some(lower), Some(upper)) = (self.ty(sub), self.ty(sup)) else {
            return false;
        };
        (lower.kind == OBJECT
            && upper.kind == UNION
            && upper.body.members.iter().any(|(m, _)| m == sub))
            || (matches!(lower.kind, OBJECT | INTERFACE)
                && upper.kind == INTERFACE
                && lower.body.interfaces.iter().any(|(i, _)| i == sup))
    }

    /// An input object must not reach itself through non-null singular
    /// input fields.
    fn check_input_cycle(&self, start: &TypeDef) -> R<()> {
        let mut stack: Vec<(&TypeDef, usize)> = vec![(start, 0)];
        let mut on_path: HashSet<&str> = HashSet::from([start.name.as_str()]);
        let mut done: HashSet<&str> = HashSet::new();
        while let Some((ty, next)) = stack.pop() {
            let Some(field) = ty.body.input_fields.get(next) else {
                on_path.remove(ty.name.as_str());
                done.insert(ty.name.as_str());
                continue;
            };
            stack.push((ty, next + 1));
            let TypeRef::NonNull(inner) = &field.ty else {
                continue;
            };
            let TypeRef::Named(target) = inner.as_ref() else {
                continue;
            };
            if target == &start.name {
                return Err(error(
                    format!("{}.{}", ty.name, field.name),
                    field.at,
                    Rule::InputCycle,
                ));
            }
            let Some(target) = self.ty(target).filter(|t| t.kind == INPUT_OBJECT) else {
                continue;
            };
            if on_path.contains(target.name.as_str()) || done.contains(target.name.as_str()) {
                continue;
            }
            on_path.insert(target.name.as_str());
            stack.push((target, 0));
        }
        Ok(())
    }

    fn check_directive_definition(&self, directive: &DirectiveDef) -> R<()> {
        let path = format!("@{}", directive.name);
        self.check_arguments(&path, &directive.arguments)?;
        if self.directive_reaches(directive, &directive.name) {
            return Err(error(path, directive.at, Rule::DirectiveSelfReference));
        }
        Ok(())
    }

    /// Whether the definition of `start` references the directive `target`
    /// directly or through the types and directives it references.
    fn directive_reaches(&self, start: &DirectiveDef, target: &str) -> bool {
        let mut directives: Vec<&DirectiveDef> = vec![start];
        let mut types: Vec<&TypeDef> = Vec::new();
        let mut seen_directives: HashSet<&str> = HashSet::new();
        let mut seen_types: HashSet<&str> = HashSet::new();
        loop {
            let mut applied: Vec<&Applied> = Vec::new();
            let mut named: Vec<&str> = Vec::new();
            if let Some(directive) = directives.pop() {
                for argument in &directive.arguments {
                    applied.extend(&argument.directives);
                    named.push(argument.ty.named());
                }
            } else if let Some(ty) = types.pop() {
                applied.extend(&ty.directives);
                for value in &ty.body.values {
                    applied.extend(&value.directives);
                }
                for field in &ty.body.input_fields {
                    applied.extend(&field.directives);
                    named.push(field.ty.named());
                }
            } else {
                return false;
            }
            for a in applied {
                if a.name == target {
                    return true;
                }
                if seen_directives.insert(a.name.as_str())
                    && let Some(d) = self.directives.iter().find(|d| d.name == a.name)
                {
                    directives.push(d);
                }
            }
            for name in named {
                if seen_types.insert(name)
                    && let Some(ty) = self.ty(name)
                {
                    types.push(ty);
                }
            }
        }
    }

    /// Every applied directive, definition by definition in the order
    /// defined: the schema, each type and its parts, each directive
    /// definition's arguments.
    fn check_applied(&self) -> R<()> {
        self.check_list(SCHEMA_VERTEX, &self.schema.directives, Location::Schema)?;
        for ty in &self.types {
            let name = ty.name.as_str();
            self.check_list(name, &ty.directives, Location::of_kind(ty.kind))?;
            for field in &ty.body.fields {
                let path = format!("{name}.{}", field.name);
                self.check_list(&path, &field.directives, Location::FieldDefinition)?;
                for argument in &field.arguments {
                    self.check_list(
                        &format!("{path}({}:)", argument.name),
                        &argument.directives,
                        Location::ArgumentDefinition,
                    )?;
                }
            }
            for value in &ty.body.values {
                self.check_list(
                    &format!("{name}.{}", value.name),
                    &value.directives,
                    Location::EnumValue,
                )?;
            }
            for field in &ty.body.input_fields {
                self.check_list(
                    &format!("{name}.{}", field.name),
                    &field.directives,
                    Location::InputFieldDefinition,
                )?;
            }
        }
        for directive in &self.directives {
            for argument in &directive.arguments {
                self.check_list(
                    &format!("@{}({}:)", directive.name, argument.name),
                    &argument.directives,
                    Location::ArgumentDefinition,
                )?;
            }
        }
        Ok(())
    }

    fn check_list(&self, path: &str, list: &[Applied], location: Location) -> R<()> {
        let mut seen: HashSet<&str> = HashSet::new();
        for applied in list {
            let Some(definition) = self.directive(&applied.name) else {
                return Err(error(path, applied.at, Rule::DirectiveUndefined));
            };
            if !definition.locations.iter().any(|l| l == location.name()) {
                return Err(error(path, applied.at, Rule::DirectiveLocation));
            }
            if !seen.insert(applied.name.as_str()) && !definition.repeatable {
                return Err(error(path, applied.at, Rule::DirectiveRepeated));
            }
            let mut given: HashSet<&str> = HashSet::new();
            for (name, value, at) in &applied.arguments {
                let Some(argument) = definition.arguments.iter().find(|a| &a.name == name) else {
                    return Err(error(path, *at, Rule::ArgumentUndefined));
                };
                if !given.insert(name.as_str()) {
                    return Err(error(path, *at, Rule::ArgumentRepeated));
                }
                if !self.coercible(value, &argument.ty, 0) {
                    return Err(error(path, *at, Rule::ValueNotCoercible));
                }
            }
            for argument in &definition.arguments {
                let value = applied
                    .arguments
                    .iter()
                    .find(|(name, _, _)| name == &argument.name);
                if required(argument) && value.is_none_or(|(_, v, _)| *v == Value::Null) {
                    return Err(error(path, applied.at, Rule::ArgumentMissing));
                }
            }
        }
        Ok(())
    }

    /// Every default value coerces to its type, and no input object's
    /// default values form a cycle.
    fn check_defaults(&self) -> R<()> {
        let check = |path: String, value: &InputValue| -> R<()> {
            if let Some((default, at)) = &value.default
                && !self.coercible(default, &value.ty, 0)
            {
                return Err(error(path, *at, Rule::ValueNotCoercible));
            }
            Ok(())
        };
        for ty in &self.types {
            for field in &ty.body.fields {
                for argument in &field.arguments {
                    check(
                        format!("{}.{}({}:)", ty.name, field.name, argument.name),
                        argument,
                    )?;
                }
            }
            for field in &ty.body.input_fields {
                check(format!("{}.{}", ty.name, field.name), field)?;
            }
        }
        for directive in &self.directives {
            for argument in &directive.arguments {
                check(format!("@{}({}:)", directive.name, argument.name), argument)?;
            }
        }
        for ty in self.types.iter().filter(|t| t.kind == INPUT_OBJECT) {
            if self.default_has_cycle(ty, &Value::Object(Vec::new()), &[]) {
                return Err(error(ty.name.clone(), ty.at, Rule::DefaultValueCycle));
            }
        }
        Ok(())
    }

    /// `InputObjectDefaultValueHasCycle(inputObject, defaultValue,
    /// visitedFields)`.
    fn default_has_cycle(&self, input: &TypeDef, value: &Value, visited: &[String]) -> bool {
        match value {
            Value::List(items) => items
                .iter()
                .any(|item| self.default_has_cycle(input, item, visited)),
            Value::Object(entries) => input.body.input_fields.iter().any(|field| {
                let Some(named) = self.ty(field.ty.named()).filter(|t| t.kind == INPUT_OBJECT)
                else {
                    return false;
                };
                if let Some((_, given)) = entries.iter().find(|(n, _)| n == &field.name) {
                    return self.default_has_cycle(named, given, visited);
                }
                let Some((default, _)) = &field.default else {
                    return false;
                };
                let key = format!("{}.{}", input.name, field.name);
                if visited.contains(&key) {
                    return true;
                }
                let mut next = visited.to_vec();
                next.push(key);
                self.default_has_cycle(named, default, &next)
            }),
            _ => false,
        }
    }

    /// Whether `value` coerces to `ty` as the input coercion rules state.
    fn coercible(&self, value: &Value, ty: &TypeRef, depth: usize) -> bool {
        if depth > MAX_NESTING {
            return false;
        }
        match ty {
            TypeRef::NonNull(inner) => {
                *value != Value::Null && self.coercible(value, inner, depth + 1)
            }
            _ if *value == Value::Null => true,
            TypeRef::List(inner) => match value {
                Value::List(items) => items
                    .iter()
                    .all(|item| self.coercible(item, inner, depth + 1)),
                _ => self.coercible(value, inner, depth + 1),
            },
            TypeRef::Named(name) => self.coercible_named(value, name, depth),
        }
    }

    fn coercible_named(&self, value: &Value, name: &str, depth: usize) -> bool {
        match name {
            "Int" => {
                matches!(value, Value::Int(text) if text.parse::<i32>().is_ok())
            }
            "Float" => match value {
                Value::Int(text) | Value::Float(text) => {
                    text.parse::<f64>().is_ok_and(f64::is_finite)
                }
                _ => false,
            },
            "String" => matches!(value, Value::Str(_)),
            "Boolean" => matches!(value, Value::Bool(_)),
            "ID" => matches!(value, Value::Str(_) | Value::Int(_)),
            _ => {
                let Some(ty) = self.ty(name) else {
                    return false;
                };
                match ty.kind {
                    ENUM => {
                        matches!(value, Value::Enum(v) if ty.body.values.iter().any(|d| &d.name == v))
                    }
                    INPUT_OBJECT => self.coercible_object(value, ty, depth),
                    _ => true,
                }
            }
        }
    }

    fn coercible_object(&self, value: &Value, ty: &TypeDef, depth: usize) -> bool {
        let Value::Object(entries) = value else {
            return false;
        };
        let mut names = HashSet::new();
        for (name, entry) in entries {
            if !names.insert(name.as_str()) {
                return false;
            }
            let Some(field) = ty.body.input_fields.iter().find(|f| &f.name == name) else {
                return false;
            };
            if !self.coercible(entry, &field.ty, depth + 1) {
                return false;
            }
        }
        for field in &ty.body.input_fields {
            let given = entries.iter().find(|(n, _)| n == &field.name);
            if required(field) && given.is_none_or(|(_, v)| *v == Value::Null) {
                return false;
            }
        }
        if ty.directives.iter().any(|d| d.name == "oneOf") {
            return entries.len() == 1 && entries.iter().all(|(_, v)| *v != Value::Null);
        }
        true
    }

    // -----------------------------------------------------------------------
    // The schema.
    // -----------------------------------------------------------------------

    fn build(&self) -> R<Schema> {
        let protocol = panproto_protocols::api::graphql::protocol();
        let mut plan = Plan::default();
        plan.vertex(SCHEMA_VERTEX, SCHEMA);
        plan.describe(SCHEMA_VERTEX, self.schema.description.as_ref());
        plan.applied(SCHEMA_VERTEX, &self.schema.directives);
        for (operation, target, _) in &self.schema.roots {
            plan.edge(SCHEMA_VERTEX, target, ROOT, Some(operation));
        }
        let mut built_in: Vec<&'static str> = Vec::new();
        let mut reference = |plan: &mut Plan, source: &str, ty: &TypeRef| {
            let named = ty.named();
            if let Some(scalar) = BUILT_IN_SCALARS.iter().find(|s| **s == named)
                && !built_in.contains(scalar)
            {
                built_in.push(scalar);
            }
            plan.constraint(source, TYPE, ty.text());
            plan.later_edge(source, named, TYPE_EDGE, None);
        };
        for ty in &self.types {
            let name = ty.name.as_str();
            plan.vertex(name, ty.kind);
            plan.describe(name, ty.description.as_ref());
            plan.applied(name, &ty.directives);
            if let Some(url) = ty
                .directives
                .iter()
                .find(|d| d.name == "specifiedBy")
                .and_then(|d| d.arguments.iter().find(|(n, _, _)| n == "url"))
                .and_then(|(_, v, _)| match v {
                    Value::Str(s) => Some(s.clone()),
                    _ => None,
                })
            {
                plan.constraint(name, SPECIFIED_BY, url);
            }
            if ty.directives.iter().any(|d| d.name == "oneOf") {
                plan.constraint(name, ONE_OF, "true".to_owned());
            }
            for (interface, _) in &ty.body.interfaces {
                plan.edge(name, interface, IMPLEMENTS, Some(interface));
            }
            for field in &ty.body.fields {
                let id = format!("{name}.{}", field.name);
                plan.vertex(&id, FIELD);
                plan.edge(name, &id, FIELD_EDGE, Some(&field.name));
                plan.describe(&id, field.description.as_ref());
                reference(&mut plan, &id, &field.ty);
                plan.applied(&id, &field.directives);
                plan.deprecation(&id, &field.directives);
                for argument in &field.arguments {
                    let arg_id = format!("{id}({}:)", argument.name);
                    plan.input_value(&arg_id, argument, &mut reference);
                    plan.edge(&id, &arg_id, ARGUMENT_EDGE, Some(&argument.name));
                }
            }
            for (member, _) in &ty.body.members {
                plan.edge(name, member, MEMBER, Some(member));
            }
            for value in &ty.body.values {
                let id = format!("{name}.{}", value.name);
                plan.vertex(&id, ENUM_VALUE);
                plan.edge(name, &id, VALUE_EDGE, Some(&value.name));
                plan.describe(&id, value.description.as_ref());
                plan.applied(&id, &value.directives);
                plan.deprecation(&id, &value.directives);
            }
            for field in &ty.body.input_fields {
                let id = format!("{name}.{}", field.name);
                plan.input_value_kind(&id, field, INPUT_FIELD, &mut reference);
                plan.edge(name, &id, INPUT_FIELD_EDGE, Some(&field.name));
            }
        }
        for directive in &self.directives {
            let id = format!("@{}", directive.name);
            plan.vertex(&id, DIRECTIVE);
            plan.describe(&id, directive.description.as_ref());
            if directive.repeatable {
                plan.constraint(&id, REPEATABLE, "true".to_owned());
            }
            for location in &directive.locations {
                plan.constraint(&id, LOCATION, location.clone());
            }
            for argument in &directive.arguments {
                let arg_id = format!("{id}({}:)", argument.name);
                plan.input_value(&arg_id, argument, &mut reference);
                plan.edge(&id, &arg_id, ARGUMENT_EDGE, Some(&argument.name));
            }
        }
        for scalar in built_in {
            plan.vertex(scalar, SCALAR);
            plan.constraint(scalar, BUILT_IN, "true".to_owned());
        }
        plan.build(&protocol).map_err(|message| GraphQlError {
            document: 0,
            path: String::new(),
            offset: 0,
            kind: GraphQlErrorKind::Build(message),
        })
    }
}

/// Add the parts of `extension` to `target`, refusing a part already
/// there.
fn merge(target: &mut TypeDef, extension: TypeDef) -> R<()> {
    let name = target.name.clone();
    let TypeDef {
        directives, body, ..
    } = extension;
    if target.kind == INPUT_OBJECT
        && let Some(one_of) = directives.iter().find(|d| d.name == "oneOf")
    {
        return Err(error(name, one_of.at, Rule::OneOfOnExtension));
    }
    target.directives.extend(directives);
    for (interface, at) in body.interfaces {
        if target.body.interfaces.iter().any(|(i, _)| *i == interface) {
            return Err(error(name, at, Rule::DuplicateInterface));
        }
        target.body.interfaces.push((interface, at));
    }
    for field in body.fields {
        if target.body.fields.iter().any(|f| f.name == field.name) {
            return Err(error(
                format!("{name}.{}", field.name),
                field.at,
                Rule::DuplicateField,
            ));
        }
        target.body.fields.push(field);
    }
    for (member, at) in body.members {
        if target.body.members.iter().any(|(m, _)| *m == member) {
            return Err(error(name, at, Rule::DuplicateMember));
        }
        target.body.members.push((member, at));
    }
    for value in body.values {
        if target.body.values.iter().any(|v| v.name == value.name) {
            return Err(error(
                format!("{name}.{}", value.name),
                value.at,
                Rule::DuplicateValue,
            ));
        }
        target.body.values.push(value);
    }
    for field in body.input_fields {
        if target
            .body
            .input_fields
            .iter()
            .any(|f| f.name == field.name)
        {
            return Err(error(
                format!("{name}.{}", field.name),
                field.at,
                Rule::DuplicateField,
            ));
        }
        target.body.input_fields.push(field);
    }
    Ok(())
}

/// The vertices, constraints and edges of the schema, in the order they
/// are added, so the edges are added after every vertex.
#[derive(Default)]
struct Plan {
    vertices: Vec<(String, &'static str)>,
    constraints: Vec<(String, &'static str, String)>,
    edges: Vec<(String, String, &'static str, Option<String>)>,
    late: Vec<(String, String, &'static str, Option<String>)>,
    unwritable: Option<String>,
}

impl Plan {
    fn vertex(&mut self, id: &str, kind: &'static str) {
        self.vertices.push((id.to_owned(), kind));
    }

    fn constraint(&mut self, id: &str, sort: &'static str, value: String) {
        self.constraints.push((id.to_owned(), sort, value));
    }

    fn edge(&mut self, src: &str, tgt: &str, kind: &'static str, name: Option<&str>) {
        self.edges.push((
            src.to_owned(),
            tgt.to_owned(),
            kind,
            name.map(str::to_owned),
        ));
    }

    /// An edge added after every other of its source: the type edge of a
    /// field, argument or input field, which follows its argument edges.
    fn later_edge(&mut self, src: &str, tgt: &str, kind: &'static str, name: Option<&str>) {
        self.late.push((
            src.to_owned(),
            tgt.to_owned(),
            kind,
            name.map(str::to_owned),
        ));
    }

    fn describe(&mut self, id: &str, description: Option<&String>) {
        if let Some(text) = description {
            self.constraint(id, DESCRIPTION, text.clone());
        }
    }

    /// The custom directives applied, one constraint each in order; the
    /// built-in ones are read into their own sorts.
    fn applied(&mut self, id: &str, directives: &[Applied]) {
        for applied in directives {
            if BUILT_IN_DIRECTIVES.contains(&applied.name.as_str()) {
                continue;
            }
            match applied_text(applied) {
                Some(text) => self.constraint(id, APPLIED, text),
                None => self.unwritable = Some(id.to_owned()),
            }
        }
    }

    fn deprecation(&mut self, id: &str, directives: &[Applied]) {
        if let Some(applied) = directives.iter().find(|d| d.name == "deprecated") {
            let reason = applied
                .arguments
                .iter()
                .find(|(n, _, _)| n == "reason")
                .and_then(|(_, v, _)| match v {
                    Value::Str(s) => Some(s.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| DEFAULT_DEPRECATION_REASON.to_owned());
            self.constraint(id, DEPRECATED, reason);
        }
    }

    fn input_value(
        &mut self,
        id: &str,
        value: &InputValue,
        reference: &mut dyn FnMut(&mut Self, &str, &TypeRef),
    ) {
        self.input_value_kind(id, value, ARGUMENT, reference);
    }

    fn input_value_kind(
        &mut self,
        id: &str,
        value: &InputValue,
        kind: &'static str,
        reference: &mut dyn FnMut(&mut Self, &str, &TypeRef),
    ) {
        self.vertex(id, kind);
        self.describe(id, value.description.as_ref());
        reference(self, id, &value.ty);
        if let Some((default, _)) = &value.default {
            match value_text(default) {
                Some(text) => self.constraint(id, DEFAULT, text),
                None => self.unwritable = Some(id.to_owned()),
            }
        }
        self.applied(id, &value.directives);
        self.deprecation(id, &value.directives);
    }

    fn build(self, protocol: &panproto_schema::Protocol) -> Result<Schema, String> {
        if let Some(id) = self.unwritable {
            return Err(format!("a string value of {id} has no GraphQL text"));
        }
        let mut builder = SchemaBuilder::new(protocol);
        for (id, kind) in &self.vertices {
            builder = builder.vertex(id, kind, None).map_err(|e| e.to_string())?;
        }
        let mut order: Vec<&(String, &'static str, String)> = self.constraints.iter().collect();
        let rank = |sort: &str| {
            SORT_ORDER
                .iter()
                .position(|s| *s == sort)
                .unwrap_or(usize::MAX)
        };
        order.sort_by_key(|(id, sort, _)| (id.clone(), rank(sort)));
        for (id, sort, value) in order {
            builder = builder.constraint(id, sort, value);
        }
        let mut edges = self.edges;
        edges.extend(self.late);
        // A schema's identity includes the order of each vertex's in-edges, so
        // the edges are added in the order of their sources' ids: the order
        // the definitions are in is then no part of the schema.
        edges.sort_by(|(left, ..), (right, ..)| left.cmp(right));
        for (src, tgt, kind, name) in &edges {
            builder = builder
                .edge(src, tgt, kind, name.as_deref())
                .map_err(|e| e.to_string())?;
        }
        builder.build().map_err(|e| e.to_string())
    }
}

/// The order of a vertex's constraints by sort; within a sort, the order
/// read.
const SORT_ORDER: [&str; 10] = [
    DESCRIPTION,
    TYPE,
    DEFAULT,
    APPLIED,
    DEPRECATED,
    SPECIFIED_BY,
    ONE_OF,
    REPEATABLE,
    LOCATION,
    BUILT_IN,
];

// ---------------------------------------------------------------------------
// The canonical writer.
// ---------------------------------------------------------------------------

/// Why [`write_graphql`] cannot write a schema.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum GraphQlWriteError {
    /// The schema has no schema vertex, or a vertex an edge names is
    /// missing.
    #[error("{0} is not a vertex of the schema")]
    Missing(String),
    /// A vertex or edge has no place in a GraphQL type system document.
    #[error("{0} has no place in a GraphQL type system document")]
    Structure(String),
    /// A string the grammar reads no text for: one holding `"""` or a
    /// carriage return, or one a block string cannot state.
    #[error("constraint {sort} of {vertex} holds a string the grammar reads no text for")]
    String {
        /// The vertex.
        vertex: String,
        /// The constraint's sort.
        sort: String,
    },
}

/// Write a `graphql` schema as its canonical GraphQL text.
///
/// The text is the schema definition, then each directive definition and
/// then each type definition in the order of their names, built-in scalars
/// left out. A definition's description is written before it, a quoted
/// string where the grammar reads one and a block string otherwise; its
/// custom directives follow it in the order read, then `@deprecated`,
/// `@specifiedBy` and `@oneOf`; its fields, arguments, values, members and
/// interfaces are in the order of its edges. Reading the text back with
/// [`read_graphql`] gives the schema it was written from.
///
/// # Errors
///
/// Returns a [`GraphQlWriteError`] for a schema no document reads as.
pub fn write_graphql(schema: &Schema) -> Result<String, GraphQlWriteError> {
    let mut out = String::new();
    let w = Writer { schema };
    if !schema.vertices.contains_key(SCHEMA_VERTEX) {
        return Err(GraphQlWriteError::Missing(SCHEMA_VERTEX.to_owned()));
    }
    out.push_str(&w.description(SCHEMA_VERTEX, "")?);
    out.push_str("schema");
    out.push_str(&w.directives(SCHEMA_VERTEX)?);
    out.push_str(" {\n");
    for edge in w.edges(SCHEMA_VERTEX, ROOT) {
        let name = edge
            .name
            .as_deref()
            .ok_or_else(|| GraphQlWriteError::Structure(SCHEMA_VERTEX.to_owned()))?;
        let _ = writeln!(out, "  {name}: {}", edge.tgt);
    }
    out.push_str("}\n");
    let mut ids: Vec<&str> = schema
        .vertices
        .iter()
        .filter(|(_, v)| v.kind.as_ref() == DIRECTIVE)
        .map(|(id, _)| id.as_ref())
        .collect();
    ids.sort_unstable();
    for id in ids {
        out.push('\n');
        out.push_str(&w.directive_definition(id)?);
    }
    let mut types: Vec<&str> = schema
        .vertices
        .iter()
        .filter(|(_, v)| TYPE_KINDS.contains(&v.kind.as_ref()))
        .map(|(id, _)| id.as_ref())
        .filter(|id| w.constraint(id, BUILT_IN).is_none())
        .collect();
    types.sort_unstable();
    for id in types {
        out.push('\n');
        out.push_str(&w.type_definition(id)?);
    }
    Ok(out)
}

/// `value` as GraphQL text, or the refusal naming the constraint it is of.
fn written_string(id: &str, sort: &str, value: &str) -> Result<String, GraphQlWriteError> {
    string_text(value).ok_or_else(|| GraphQlWriteError::String {
        vertex: id.to_owned(),
        sort: sort.to_owned(),
    })
}

struct Writer<'s> {
    schema: &'s Schema,
}

impl Writer<'_> {
    fn constraints(&self, id: &str, sort: &str) -> Vec<&str> {
        self.schema
            .constraints
            .get(id)
            .map(|list| {
                list.iter()
                    .filter(|c| c.sort.as_ref() == sort)
                    .map(|c| c.value.as_str())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn constraint(&self, id: &str, sort: &str) -> Option<&str> {
        self.constraints(id, sort).into_iter().next()
    }

    fn edges(&self, id: &str, kind: &str) -> Vec<&panproto_schema::Edge> {
        self.schema
            .outgoing_edges(id)
            .iter()
            .filter(|e| e.kind.as_ref() == kind)
            .collect()
    }

    /// The description of `id`, on its own line at `indent`, or nothing.
    fn description(&self, id: &str, indent: &str) -> Result<String, GraphQlWriteError> {
        Ok(match self.constraint(id, DESCRIPTION) {
            Some(text) => format!(
                "{indent}{}\n{indent}",
                written_string(id, DESCRIPTION, text)?
            ),
            None => String::new(),
        })
    }

    /// The directives applied to `id`, each after a space.
    fn directives(&self, id: &str) -> Result<String, GraphQlWriteError> {
        let mut out = String::new();
        for applied in self.constraints(id, APPLIED) {
            out.push(' ');
            out.push_str(applied);
        }
        if let Some(reason) = self.constraint(id, DEPRECATED) {
            let text = written_string(id, DEPRECATED, reason)?;
            let _ = write!(out, " @deprecated(reason: {text})");
        }
        if let Some(url) = self.constraint(id, SPECIFIED_BY) {
            let text = written_string(id, SPECIFIED_BY, url)?;
            let _ = write!(out, " @specifiedBy(url: {text})");
        }
        if self.constraint(id, ONE_OF).is_some() {
            out.push_str(" @oneOf");
        }
        Ok(out)
    }

    fn type_text(&self, id: &str) -> Result<&str, GraphQlWriteError> {
        self.constraint(id, TYPE)
            .ok_or_else(|| GraphQlWriteError::Structure(id.to_owned()))
    }

    fn kind(&self, id: &str) -> Result<&str, GraphQlWriteError> {
        self.schema
            .vertices
            .get(id)
            .map(|v| v.kind.as_ref())
            .ok_or_else(|| GraphQlWriteError::Missing(id.to_owned()))
    }

    /// An argument or input field: `name: Type = default @directives`.
    fn input_value(&self, edge: &panproto_schema::Edge) -> Result<String, GraphQlWriteError> {
        let id = edge.tgt.as_ref();
        self.kind(id)?;
        let name = edge
            .name
            .as_deref()
            .ok_or_else(|| GraphQlWriteError::Structure(id.to_owned()))?;
        let mut out = self.description(id, "")?;
        let _ = write!(out, "{name}: {}", self.type_text(id)?);
        if let Some(default) = self.constraint(id, DEFAULT) {
            let _ = write!(out, " = {default}");
        }
        out.push_str(&self.directives(id)?);
        Ok(out)
    }

    fn arguments(&self, id: &str) -> Result<String, GraphQlWriteError> {
        let arguments = self.edges(id, ARGUMENT_EDGE);
        if arguments.is_empty() {
            return Ok(String::new());
        }
        let list: Result<Vec<String>, _> = arguments.iter().map(|e| self.input_value(e)).collect();
        Ok(format!("({})", list?.join(", ")))
    }

    fn directive_definition(&self, id: &str) -> Result<String, GraphQlWriteError> {
        let mut out = self.description(id, "")?;
        let _ = write!(out, "directive {id}{}", self.arguments(id)?);
        if self.constraint(id, REPEATABLE).is_some() {
            out.push_str(" repeatable");
        }
        let _ = writeln!(out, " on {}", self.constraints(id, LOCATION).join(" | "));
        Ok(out)
    }

    fn type_definition(&self, id: &str) -> Result<String, GraphQlWriteError> {
        let kind = self.kind(id)?;
        let keyword = match kind {
            SCALAR => "scalar",
            OBJECT => "type",
            INTERFACE => "interface",
            UNION => "union",
            ENUM => "enum",
            _ => "input",
        };
        let mut out = self.description(id, "")?;
        let _ = write!(out, "{keyword} {id}");
        let interfaces: Vec<&str> = self
            .edges(id, IMPLEMENTS)
            .iter()
            .map(|e| e.tgt.as_ref())
            .collect();
        if !interfaces.is_empty() {
            let _ = write!(out, " implements {}", interfaces.join(" & "));
        }
        out.push_str(&self.directives(id)?);
        match kind {
            OBJECT | INTERFACE => {
                out.push_str(" {\n");
                for edge in self.edges(id, FIELD_EDGE) {
                    let field = edge.tgt.as_ref();
                    self.kind(field)?;
                    let name = edge
                        .name
                        .as_deref()
                        .ok_or_else(|| GraphQlWriteError::Structure(field.to_owned()))?;
                    out.push_str(&self.description(field, "  ")?);
                    if self.constraint(field, DESCRIPTION).is_none() {
                        out.push_str("  ");
                    }
                    let _ = writeln!(
                        out,
                        "{name}{}: {}{}",
                        self.arguments(field)?,
                        self.type_text(field)?,
                        self.directives(field)?
                    );
                }
                out.push_str("}\n");
            }
            UNION => {
                let members: Vec<&str> = self
                    .edges(id, MEMBER)
                    .iter()
                    .map(|e| e.tgt.as_ref())
                    .collect();
                if !members.is_empty() {
                    let _ = write!(out, " = {}", members.join(" | "));
                }
                out.push('\n');
            }
            ENUM => {
                out.push_str(" {\n");
                for edge in self.edges(id, VALUE_EDGE) {
                    let value = edge.tgt.as_ref();
                    self.kind(value)?;
                    let name = edge
                        .name
                        .as_deref()
                        .ok_or_else(|| GraphQlWriteError::Structure(value.to_owned()))?;
                    out.push_str(&self.description(value, "  ")?);
                    if self.constraint(value, DESCRIPTION).is_none() {
                        out.push_str("  ");
                    }
                    let _ = writeln!(out, "{name}{}", self.directives(value)?);
                }
                out.push_str("}\n");
            }
            INPUT_OBJECT => {
                out.push_str(" {\n");
                for edge in self.edges(id, INPUT_FIELD_EDGE) {
                    let _ = writeln!(out, "  {}", self.input_value(edge)?);
                }
                out.push_str("}\n");
            }
            _ => out.push('\n'),
        }
        Ok(out)
    }
}

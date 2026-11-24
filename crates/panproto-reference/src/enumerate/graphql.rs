//! Complete finite scopes for the GraphQL page.
//!
//! The document scope is the cartesian product of nine choices made on one
//! base type system: a field added to the object type `Pet`
//! ([`PET_FIELDS`]), the interfaces `Pet` implements with the type of its
//! `id` ([`IMPLEMENTS`]), the members of the union `Result` ([`MEMBERS`]),
//! the default value of the argument `Query.search(filter:)`
//! ([`DEFAULTS`]), the directives applied to `Pet` ([`PET_DIRECTIVES`]),
//! one extension ([`EXTENSIONS`]), the schema definition ([`SCHEMAS`]),
//! the descriptions ([`DESCRIPTIONS`]), and the form of the input object
//! `Filter` ([`FILTERS`]). The first choices of each are read; the rest
//! each break one rule.

use crate::pages::graphql::{Def, Dir, EnumVal, Field, Input, Kind, Ty, TypeDef, Val, l, n, nn};

/// The sizes of the nine choices and how many of each are read.
pub const PET_FIELDS: (usize, usize) = (4, 2);
/// See [`PET_FIELDS`].
pub const IMPLEMENTS: (usize, usize) = (3, 1);
/// See [`PET_FIELDS`].
pub const MEMBERS: (usize, usize) = (2, 1);
/// See [`PET_FIELDS`].
pub const DEFAULTS: (usize, usize) = (4, 3);
/// See [`PET_FIELDS`].
pub const PET_DIRECTIVES: (usize, usize) = (4, 2);
/// See [`PET_FIELDS`].
pub const EXTENSIONS: (usize, usize) = (4, 3);
/// See [`PET_FIELDS`].
pub const SCHEMAS: (usize, usize) = (4, 3);
/// See [`PET_FIELDS`].
pub const DESCRIPTIONS: (usize, usize) = (2, 2);
/// See [`PET_FIELDS`].
pub const FILTERS: (usize, usize) = (3, 2);

/// The nine choices in order.
pub const CHOICES: [(usize, usize); 9] = [
    PET_FIELDS,
    IMPLEMENTS,
    MEMBERS,
    DEFAULTS,
    PET_DIRECTIVES,
    EXTENSIONS,
    SCHEMAS,
    DESCRIPTIONS,
    FILTERS,
];

fn dir(name: &str, args: &[(&str, Val)]) -> Dir {
    Dir {
        name: name.to_owned(),
        args: args
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
    }
}

fn string(text: &str) -> Val {
    Val::Str(text.to_owned())
}

fn input(name: &str, ty: Ty, default: Option<Val>, dirs: Vec<Dir>) -> Input {
    Input {
        desc: None,
        name: name.to_owned(),
        ty,
        default,
        dirs,
    }
}

fn field(name: &str, args: Vec<Input>, ty: Ty, dirs: Vec<Dir>) -> Field {
    Field {
        desc: None,
        name: name.to_owned(),
        args,
        ty,
        dirs,
    }
}

fn value(name: &str) -> EnumVal {
    EnumVal {
        desc: None,
        name: name.to_owned(),
        dirs: Vec::new(),
    }
}

/// The document of one choice of each of the nine.
#[must_use]
pub fn document(choice: [usize; 9]) -> Vec<Def> {
    let [
        pet_field,
        implements,
        members,
        default,
        pet_dirs,
        extension,
        schema,
        described,
        filter,
    ] = choice;
    let mut query = TypeDef::new(Kind::Object, "Query");
    query.fields = vec![
        field(
            "node",
            vec![input("id", nn(n("ID")), None, Vec::new())],
            n("Node"),
            Vec::new(),
        ),
        field(
            "search",
            vec![input(
                "filter",
                n("Filter"),
                match default {
                    0 => None,
                    1 => Some(Val::Obj(vec![("text".to_owned(), string("a"))])),
                    2 => Some(Val::Obj(vec![(
                        "kind".to_owned(),
                        Val::Enum("CAT".to_owned()),
                    )])),
                    _ => Some(Val::Obj(vec![(
                        "kind".to_owned(),
                        Val::Enum("BIRD".to_owned()),
                    )])),
                },
                Vec::new(),
            )],
            l(n("Result")),
            Vec::new(),
        ),
    ];
    let mut node = TypeDef::new(Kind::Interface, "Node");
    node.fields = vec![field("id", Vec::new(), nn(n("ID")), Vec::new())];

    let mut pet = TypeDef::new(Kind::Object, "Pet");
    let (interfaces, id) = match implements {
        0 => (vec!["Node"], nn(n("ID"))),
        1 => (vec!["Node"], n("ID")),
        _ => (vec!["Query"], nn(n("ID"))),
    };
    pet.implements = interfaces.into_iter().map(str::to_owned).collect();
    let mut name = field("name", Vec::new(), n("String"), Vec::new());
    if described == 1 {
        name.desc = Some("first\n  second".to_owned());
        query.desc = Some("The query type.".to_owned());
    }
    pet.fields = vec![field("id", Vec::new(), id, Vec::new()), name];
    pet.fields.extend(added_pet_field(pet_field));
    pet.dirs = match pet_dirs {
        0 => Vec::new(),
        1 => vec![dir("tag", &[("name", string("x"))])],
        2 => vec![dir("tag", &[])],
        _ => vec![dir("deprecated", &[])],
    };

    let mut result = TypeDef::new(Kind::Union, "Result");
    result.members = vec![if members == 0 { "Pet" } else { "Node" }.to_owned()];

    let mut kind = TypeDef::new(Kind::Enum, "Kind");
    kind.values = vec![value("CAT"), value("DOG")];

    let mut filter_type = TypeDef::new(Kind::Input, "Filter");
    filter_type.inputs = vec![
        input("text", n("String"), None, Vec::new()),
        input("kind", n("Kind"), None, Vec::new()),
    ];
    match filter {
        0 => {}
        1 => filter_type.dirs = vec![dir("oneOf", &[])],
        _ => filter_type
            .inputs
            .push(input("self", nn(n("Filter")), None, Vec::new())),
    }

    let mut doc: Vec<Def> = schema_definition(schema).into_iter().collect();
    doc.push(Def::Directive {
        desc: Some("A tag.".to_owned()),
        name: "tag".to_owned(),
        args: vec![input("name", nn(n("String")), None, Vec::new())],
        repeatable: false,
        locations: vec!["OBJECT".to_owned(), "FIELD_DEFINITION".to_owned()],
    });
    doc.extend([
        Def::Type(query),
        Def::Type(node),
        Def::Type(pet),
        Def::Type(result),
        Def::Type(kind),
        Def::Type(filter_type),
    ]);
    doc.extend(extension_definition(extension));
    doc
}

/// The field the choice `pet_field` adds to `Pet`.
fn added_pet_field(pet_field: usize) -> Option<Field> {
    match pet_field {
        0 => None,
        1 => Some(field(
            "owner",
            vec![input(
                "first",
                n("Int"),
                Some(Val::Int("10".to_owned())),
                vec![dir("deprecated", &[])],
            )],
            n("Pet"),
            vec![
                dir("tag", &[("name", string("o"))]),
                dir("deprecated", &[("reason", string("Use \"keeper\"."))]),
            ],
        )),
        2 => Some(field("__x", Vec::new(), n("Int"), Vec::new())),
        _ => Some(field("age", Vec::new(), n("Filter"), Vec::new())),
    }
}

/// The schema definition of the choice `schema`.
fn schema_definition(schema: usize) -> Option<Def> {
    match schema {
        0 => None,
        1 => Some(Def::Schema {
            desc: None,
            dirs: Vec::new(),
            roots: vec![("query".to_owned(), "Query".to_owned())],
        }),
        2 => Some(Def::Schema {
            desc: Some("S".to_owned()),
            dirs: Vec::new(),
            roots: vec![
                ("query".to_owned(), "Query".to_owned()),
                ("mutation".to_owned(), "Pet".to_owned()),
            ],
        }),
        _ => Some(Def::Schema {
            desc: None,
            dirs: Vec::new(),
            roots: vec![("query".to_owned(), "Result".to_owned())],
        }),
    }
}

/// The extension of the choice `extension`.
fn extension_definition(extension: usize) -> Option<Def> {
    match extension {
        0 => None,
        1 => {
            let mut ext = TypeDef::new(Kind::Object, "Pet");
            ext.fields = vec![field("legs", Vec::new(), n("Int"), Vec::new())];
            Some(Def::Extend(ext))
        }
        2 => {
            let mut ext = TypeDef::new(Kind::Enum, "Kind");
            ext.values = vec![value("FISH")];
            Some(Def::Extend(ext))
        }
        _ => {
            let mut ext = TypeDef::new(Kind::Object, "Kind");
            ext.fields = vec![field("a", Vec::new(), n("Int"), Vec::new())];
            Some(Def::Extend(ext))
        }
    }
}

/// Every choice of the scope, in lexicographic order.
#[must_use]
pub fn choices() -> Vec<[usize; 9]> {
    let mut out = vec![[0; 9]];
    for (i, (size, _)) in CHOICES.iter().enumerate() {
        out = out
            .into_iter()
            .flat_map(|c| {
                (0..*size).map(move |k| {
                    let mut next = c;
                    next[i] = k;
                    next
                })
            })
            .collect();
    }
    out
}

/// Every document of the scope.
#[must_use]
pub fn documents() -> Vec<Vec<Def>> {
    choices().into_iter().map(document).collect()
}

/// The number of documents of the scope, the product of the nine choices'
/// sizes: $4 \cdot 3 \cdot 2 \cdot 4 \cdot 4 \cdot 4 \cdot 4 \cdot 2 \cdot
/// 3 = 36864$.
#[must_use]
pub const fn document_count() -> usize {
    let mut product = 1;
    let mut i = 0;
    while i < CHOICES.len() {
        product *= CHOICES[i].0;
        i += 1;
    }
    product
}

/// The number of documents the page reads: those whose nine choices are
/// each read, $2 \cdot 1 \cdot 1 \cdot 3 \cdot 2 \cdot 3 \cdot 3 \cdot 2
/// \cdot 2 = 432$.
#[must_use]
pub const fn accepted_count() -> usize {
    let mut product = 1;
    let mut i = 0;
    while i < CHOICES.len() {
        product *= CHOICES[i].1;
        i += 1;
    }
    product
}

//! Complete finite scopes for the cloudformation page.
//!
//! The document scope is a base template whose resource `S` holds zero, one
//! or two of the properties `a` and `b` (in either order), each one of the
//! [`values`], together with the [`envelopes`]: the base template with one
//! property `a` holding a string, changed in one place outside the
//! properties.

use crate::pages::cloudformation::{Doc, m, s};

const fn seq(items: Vec<Doc>) -> Doc {
    Doc::Seq(items)
}

fn reference(name: &str) -> Doc {
    m(&[("Ref", s(name))])
}

/// The values a property of the scope may hold.
///
/// They are a string, references that resolve to a parameter, a resource and a pseudo parameter, one that
/// resolves to nothing, attribute reads in both forms and to nothing, two
/// substitutions, a map lookup, a conditional value, an import, a join with
/// one argument, a language extension, an unknown function and a
/// conditional value naming no condition.
#[must_use]
pub fn values() -> Vec<Doc> {
    vec![
        s("x"),
        reference("P"),
        reference("R"),
        reference("AWS::Region"),
        reference("Z"),
        m(&[("Fn::GetAtt", seq(vec![s("R"), s("Arn")]))]),
        m(&[("Fn::GetAtt", s("R.Arn"))]),
        m(&[("Fn::GetAtt", seq(vec![s("Z"), s("Arn")]))]),
        m(&[("Fn::Sub", s("${P}-${R.Arn}-${!x}"))]),
        m(&[(
            "Fn::Sub",
            seq(vec![s("${v}-${AWS::Region}"), m(&[("v", reference("P"))])]),
        )]),
        m(&[("Fn::FindInMap", seq(vec![s("M"), s("k"), s("v")]))]),
        m(&[("Fn::If", seq(vec![s("C"), reference("P"), s("y")]))]),
        m(&[("Fn::ImportValue", s("e"))]),
        m(&[("Fn::Join", seq(vec![s("-")]))]),
        m(&[("Fn::Length", seq(vec![Doc::Int(1)]))]),
        m(&[("Fn::Bogus", Doc::Int(1))]),
        m(&[("Fn::If", seq(vec![s("Z"), Doc::Int(1), Doc::Int(2)]))]),
    ]
}

/// The parts of a template the scope varies.
struct Parts {
    header: Vec<(&'static str, Doc)>,
    parameters: Vec<(&'static str, Doc)>,
    mappings: Doc,
    conditions: Vec<(&'static str, Doc)>,
    resource_r: Doc,
    resource_s: Vec<(&'static str, Doc)>,
    outputs: Option<Doc>,
    resources: Option<Vec<(&'static str, Doc)>>,
}

impl Parts {
    fn base(properties: Option<Doc>) -> Self {
        let mut resource_s = vec![("Type", s("AWS::SNS::Topic"))];
        if let Some(p) = properties {
            resource_s.push(("Properties", p));
        }
        Self {
            header: Vec::new(),
            parameters: vec![("P", m(&[("Type", s("String"))]))],
            mappings: m(&[("M", m(&[("k", m(&[("v", s("x"))]))]))]),
            conditions: vec![("C", m(&[("Fn::Equals", seq(vec![reference("P"), s("a")]))]))],
            resource_r: m(&[("Type", s("AWS::S3::Bucket"))]),
            resource_s,
            outputs: None,
            resources: None,
        }
    }

    fn doc(self) -> Doc {
        let mut top = self.header;
        top.push(("Parameters", m(&self.parameters)));
        top.push(("Mappings", self.mappings));
        top.push(("Conditions", m(&self.conditions)));
        let resources = self
            .resources
            .unwrap_or_else(|| vec![("R", self.resource_r), ("S", m(&self.resource_s))]);
        top.push(("Resources", m(&resources)));
        if let Some(outputs) = self.outputs {
            top.push(("Outputs", outputs));
        }
        m(&top)
    }
}

/// The base template with the properties `props` on `S`, none when empty.
#[must_use]
pub fn base(props: &[(&str, Doc)]) -> Doc {
    Parts::base((!props.is_empty()).then(|| m(props))).doc()
}

fn one() -> Parts {
    Parts::base(Some(m(&[("a", s("x"))])))
}

/// The base template with property `a` holding `"x"`, changed in one place
/// outside the properties.
#[must_use]
pub fn envelopes() -> Vec<Doc> {
    let mut out = header_envelopes();
    out.extend(declaration_envelopes());
    out
}

fn header_envelopes() -> Vec<Doc> {
    let with_header = |key: &'static str, value: Doc| {
        let mut p = one();
        p.header.push((key, value));
        p.doc()
    };
    let mut out = vec![
        with_header("AWSTemplateFormatVersion", s("2010-09-09")),
        with_header("AWSTemplateFormatVersion", s("2011-01-01")),
        with_header("Transform", s("AWS::Serverless-2016-10-31")),
        with_header("Rules", m(&[])),
        with_header("Foo", s("x")),
        with_header("Metadata", s("x")),
        with_header("Description", Doc::Int(5)),
    ];
    let mut no_resources = one();
    no_resources.resources = Some(Vec::new());
    out.push(no_resources.doc());
    let Doc::Map(mut entries) = one().doc() else {
        return out;
    };
    entries.retain(|(k, _)| k != "Resources");
    out.push(Doc::Map(entries));
    let mut renamed = one();
    renamed.resources = Some(vec![
        ("R", m(&[("Type", s("AWS::S3::Bucket"))])),
        ("S-1", m(&[("Type", s("AWS::SNS::Topic"))])),
    ]);
    out.push(renamed.doc());
    out
}

fn declaration_envelopes() -> Vec<Doc> {
    let parameter = |body: Doc, name: &'static str| {
        let mut p = one();
        p.parameters.push((name, body));
        p.doc()
    };
    let s_field = |key: &'static str, value: Doc| {
        let mut p = one();
        p.resource_s.push((key, value));
        p.doc()
    };
    let r_type = |ty: &str| {
        let mut p = one();
        p.resource_r = m(&[("Type", s(ty))]);
        p.doc()
    };
    let condition = |body: Doc| {
        let mut p = one();
        p.conditions = vec![("C", body)];
        p.doc()
    };
    let output = |body: Doc| {
        let mut p = one();
        p.outputs = Some(m(&[("O", body)]));
        p.doc()
    };
    let mut mapped = one();
    mapped.mappings = m(&[("M", m(&[("k", m(&[("v", m(&[("x", Doc::Int(1))]))]))]))]);
    vec![
        parameter(m(&[("Type", s("String"))]), "R"),
        parameter(m(&[("Default", s("d"))]), "Q"),
        parameter(m(&[("Type", s("Strng"))]), "Q"),
        parameter(m(&[("Type", s("String")), ("Foo", s("x"))]), "Q"),
        r_type("S3Bucket"),
        r_type("A::B::C::MODULE"),
        s_field("Condition", s("C")),
        s_field("Condition", s("Z")),
        s_field("DependsOn", s("R")),
        s_field("DependsOn", seq(vec![s("R"), s("Z")])),
        s_field("DeletionPolicy", s("Keep")),
        condition(m(&[("Fn::Equals", seq(vec![reference("R"), s("a")]))])),
        condition(m(&[("a", s("b"))])),
        output(m(&[
            ("Value", m(&[("Fn::GetAtt", seq(vec![s("R"), s("Arn")]))])),
            ("Export", m(&[("Name", s("n"))])),
            ("Condition", s("C")),
        ])),
        output(m(&[("Description", s("d"))])),
        mapped.doc(),
    ]
}

/// The documents of the scope: the base template with no property, each
/// value as `a` and as `b`, each ordered pair of values as `a` then `b`
/// and as `b` then `a`, then the envelopes.
#[must_use]
pub fn documents() -> Vec<Doc> {
    let values = values();
    let mut out = vec![base(&[])];
    for key in ["a", "b"] {
        for v in &values {
            out.push(base(&[(key, v.clone())]));
        }
    }
    for (first, second) in [("a", "b"), ("b", "a")] {
        for v in &values {
            for w in &values {
                out.push(base(&[(first, v.clone()), (second, w.clone())]));
            }
        }
    }
    out.extend(envelopes());
    out
}

/// The closed form of the scope's size for `v` values and `e` envelopes:
/// `1 + 2v + 2v² + e`.
#[must_use]
pub const fn document_count(v: usize, e: usize) -> usize {
    1 + 2 * v + 2 * v * v + e
}

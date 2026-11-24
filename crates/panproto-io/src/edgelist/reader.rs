//! The edge-list reader and the `edgelist-csv` and `edgelist-tsv` graph
//! codecs.

use panproto_inst::GInstance;
use panproto_inst::value::Value;
use panproto_parse::languages::common::LanguageParser;
use panproto_parse::registry::AstParser;
use panproto_schema::{Schema, SchemaBuilder};

use super::{Dialect, Edge, EdgeListReadError, from_graph, schema_holds_encoding, to_graph, write};
use crate::cst_extract::{extract_tabular_cst_with_protocol, tabular_header};
use crate::error::{EmitInstanceError, ParseInstanceError, UnifiedCodecError};
use crate::traits::{GraphInstanceEmitter, GraphInstanceParser};
use crate::unified_codec::language_parser;

/// The table vertex the delimited text is read into.
const ROWS: &str = "rows";

/// An edge-list graph codec: reads an edge list of its dialect into the
/// edge-list graph encoding and writes a graph in that encoding as the
/// canonical edge list.
///
/// The text is read by the generic CSV or TSV route: the bundled
/// tree-sitter CSV or TSV grammar parses it, and the route's table
/// extraction reads its records, so the edge list has exactly the quoting,
/// record and field-count rules of that route.
pub struct EdgeListCodec {
    dialect: Dialect,
    parser: LanguageParser,
    table_protocol: panproto_schema::Protocol,
    table_schema: Schema,
}

impl EdgeListCodec {
    /// A codec for `dialect` over the bundled tree-sitter CSV or TSV
    /// grammar.
    ///
    /// # Errors
    ///
    /// Returns [`UnifiedCodecError`] if the grammar is not compiled in or
    /// fails to initialise.
    pub fn new(dialect: Dialect) -> Result<Self, UnifiedCodecError> {
        let (grammar, table_protocol) = match dialect {
            Dialect::Csv => ("csv", panproto_protocols::text::csv::protocol()),
            Dialect::Tsv => ("tsv", panproto_protocols::text::tsv::protocol()),
        };
        let table_schema = SchemaBuilder::new(&table_protocol)
            .vertex(ROWS, "table", None)
            .and_then(|builder| builder.entry(ROWS).build())
            .map_err(|error| UnifiedCodecError::SchemaInit {
                format: grammar.to_owned(),
                source: Box::new(error),
            })?;
        Ok(Self {
            dialect,
            parser: language_parser(grammar)?,
            table_protocol,
            table_schema,
        })
    }

    /// The codec's dialect.
    #[must_use]
    pub const fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// The edges of an edge-list document, one per data record in record
    /// order.
    ///
    /// # Errors
    ///
    /// Returns [`EdgeListReadError`] for the first fault, checked in this
    /// order: invalid UTF-8; text the grammar cannot read (`Table`); no
    /// header record; each header field left to right (`UnsupportedColumn`,
    /// `DuplicateColumn`); a missing `source`, then `target` column; a
    /// record whose field count differs from the header's (`Table`); and
    /// then, record by record, an empty `source`, then `target` field.
    pub fn read(&self, input: &[u8]) -> Result<Vec<Edge>, EdgeListReadError> {
        std::str::from_utf8(input).map_err(|error| EdgeListReadError::NotUtf8 {
            offset: error.valid_up_to(),
        })?;
        let table = |message: String| EdgeListReadError::Table { message };
        let file_name = match self.dialect {
            Dialect::Csv => "input.csv",
            Dialect::Tsv => "input.tsv",
        };
        let cst = self
            .parser
            .parse(input, file_name)
            .map_err(|error| table(error.to_string()))?;
        let header = tabular_header(&cst)
            .map_err(|error| table(error.to_string()))?
            .ok_or(EdgeListReadError::NoHeader)?;
        let (source_column, target_column, label_column) = columns(&header)?;
        let (instance, _) =
            extract_tabular_cst_with_protocol(&cst, &self.table_schema, &self.table_protocol, ROWS)
                .map_err(|error| table(error.to_string()))?;
        instance
            .rows(ROWS)
            .iter()
            .enumerate()
            .map(|(index, (_, row))| {
                let field = |name: &str| match row.get(name) {
                    Some(Value::Str(text)) => text.clone(),
                    _ => String::new(),
                };
                let source = field(source_column);
                let target = field(target_column);
                for (name, text) in [("source", &source), ("target", &target)] {
                    if text.is_empty() {
                        return Err(EdgeListReadError::EmptyName {
                            record: index + 1,
                            column: name.to_owned(),
                        });
                    }
                }
                Ok(Edge {
                    source,
                    target,
                    label: label_column.map(field),
                })
            })
            .collect()
    }
}

/// The header fields that name the `source`, `target` and `label` columns,
/// checked as [`EdgeListCodec::read`] states.
fn columns(header: &[String]) -> Result<(&str, &str, Option<&str>), EdgeListReadError> {
    const NAMES: [&str; 3] = ["source", "target", "label"];
    let mut found: [Option<&str>; 3] = [None; 3];
    for (column, name) in header.iter().enumerate() {
        let Some(slot) = NAMES.iter().position(|n| n.eq_ignore_ascii_case(name)) else {
            return Err(EdgeListReadError::UnsupportedColumn {
                column,
                name: name.clone(),
            });
        };
        if found[slot].is_some() {
            return Err(EdgeListReadError::DuplicateColumn {
                column,
                name: name.clone(),
            });
        }
        found[slot] = Some(name);
    }
    let missing = |name: &str| EdgeListReadError::MissingColumn {
        name: name.to_owned(),
    };
    let source = found[0].ok_or_else(|| missing("source"))?;
    let target = found[1].ok_or_else(|| missing("target"))?;
    Ok((source, target, found[2]))
}

fn schema_mismatch() -> String {
    "the schema is not the edge-list graph encoding".into()
}

impl GraphInstanceParser for EdgeListCodec {
    fn protocol_name(&self) -> &'static str {
        self.dialect.protocol()
    }

    fn parse_graph(&self, schema: &Schema, input: &[u8]) -> Result<GInstance, ParseInstanceError> {
        let protocol = self.dialect.protocol();
        if !schema_holds_encoding(schema) {
            return Err(ParseInstanceError::SchemaMismatch {
                protocol: protocol.into(),
                message: schema_mismatch(),
            });
        }
        let edges = self
            .read(input)
            .map_err(|error| ParseInstanceError::EdgeList {
                protocol: protocol.into(),
                error,
            })?;
        to_graph(&edges).map_err(|e| ParseInstanceError::Parse {
            protocol: protocol.into(),
            message: e.to_string(),
        })
    }
}

impl GraphInstanceEmitter for EdgeListCodec {
    fn protocol_name(&self) -> &'static str {
        self.dialect.protocol()
    }

    fn emit_graph(
        &self,
        schema: &Schema,
        instance: &GInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        let protocol = self.dialect.protocol();
        if !schema_holds_encoding(schema) {
            return Err(EmitInstanceError::Emit {
                protocol: protocol.into(),
                message: schema_mismatch(),
            });
        }
        let edges =
            from_graph(instance, self.dialect).map_err(|error| EmitInstanceError::EdgeList {
                protocol: protocol.into(),
                error,
            })?;
        write(&edges, self.dialect)
            .map(String::into_bytes)
            .ok_or_else(|| EmitInstanceError::Emit {
                protocol: protocol.into(),
                message: "the checked edges have no canonical text".into(),
            })
    }
}

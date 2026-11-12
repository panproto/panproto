"""Schema documents of the protocols read through a tree-sitter grammar.

``parse_schema_document`` and ``parse_schema_source`` reach the readers of
``panproto-io`` by protocol name. Each case reads a document of the
protocol's conformance corpus, checks it against the same document read
by the other route, and checks the refusals a caller can meet.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

import panproto

CORPUS = (
    Path(__file__).resolve().parents[3]
    / "crates"
    / "panproto-io"
    / "tests"
    / "fixtures"
    / "conformance"
)


def _text(relative: str) -> str:
    return (CORPUS / relative).read_text(encoding="utf-8")


@pytest.mark.parametrize(
    ("protocol", "relative", "ansible_class"),
    [
        ("openapi", "openapi/upstream/pass/minimal_paths.yaml", None),
        ("asyncapi", "asyncapi/upstream/examples/simple-asyncapi.yml", None),
        ("k8s-crd", "k8s-crd/upstream/crd.yaml", None),
        ("cloudformation", "cloudformation/upstream/SNS_SNSTopic.json", None),
        (
            "ansible",
            "ansible/upstream/tasks/includes_provisioning_tasks_apache.yml",
            "tasks",
        ),
        ("postgresql", "postgresql-docs/cases/create-table-examples-02.sql", None),
    ],
)
def test_a_corpus_document_is_read_by_its_reader(
    protocol: str, relative: str, ansible_class: str | None
) -> None:
    schema = panproto.parse_schema_source(protocol, _text(relative), ansible_class=ansible_class)
    assert schema.protocol == protocol
    assert schema.vertex_count > 0


def test_a_json_document_reads_the_same_by_both_routes() -> None:
    template = _text("cloudformation/upstream/SNS_SNSTopic.json")
    as_text = panproto.parse_schema_document("cloudformation", template)
    as_dict = panproto.parse_schema_document("cloudformation", json.loads(template))
    as_source = panproto.parse_schema_source("cloudformation", template)
    assert as_text.to_json() == as_source.to_json()
    assert as_dict.to_json() == as_source.to_json()


def test_a_json_schema_from_the_test_suite_is_read() -> None:
    cases = json.loads(_text("json-schema-test-suite/tests/minLength.json"))
    schema = panproto.parse_schema_document("json-schema", cases["tests"][0]["schema"])
    assert schema.protocol == "json-schema"


def test_a_json_schema_bundle_resolves_between_its_documents() -> None:
    entry = {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://example.com/a",
        "$ref": "b",
    }
    target = {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://example.com/b",
        "type": "string",
    }
    schema = panproto.parse_schema_bundle("json-schema", [entry, target])
    assert schema.protocol == "json-schema"
    assert "json-schema" in panproto.list_bundle_parser_protocols()


def test_an_ansible_file_needs_its_class() -> None:
    source = _text("ansible/upstream/tasks/includes_provisioning_tasks_apache.yml")
    with pytest.raises(ValueError, match="needs the file's class"):
        panproto.parse_schema_source("ansible", source)
    with pytest.raises(ValueError, match="unknown Ansible file class"):
        panproto.parse_schema_source("ansible", source, ansible_class="role")


def test_a_reader_refusal_names_what_is_wrong() -> None:
    with pytest.raises(ValueError, match=r"Swagger 2\.0 documents are not read"):
        panproto.parse_schema_document("openapi", {"swagger": "2.0"})


def test_a_postgresql_script_is_not_a_json_document() -> None:
    with pytest.raises(ValueError, match="read from source text"):
        panproto.parse_schema_document("postgresql", {"tables": []})


def test_bson_reads_no_schema_documents() -> None:
    with pytest.raises(ValueError, match="reads no schema documents"):
        panproto.parse_schema_document("bson", {"$jsonSchema": {"bsonType": "object"}})

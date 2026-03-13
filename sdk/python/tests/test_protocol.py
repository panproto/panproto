"""Tests for the five built-in panproto protocol specifications."""

from __future__ import annotations

import panproto

# ---------------------------------------------------------------------------
# ATPROTO_SPEC
# ---------------------------------------------------------------------------


class TestAtprotoSpec:
    """Tests for the ATProto built-in protocol specification."""

    def test_name(self) -> None:
        """Verify the protocol name is 'atproto'.

        Parameters
        ----------
        None
        """
        assert panproto.ATPROTO_SPEC["name"] == "atproto"

    def test_schema_theory(self) -> None:
        """Verify the schema theory is ThConstrainedMultiGraph.

        Parameters
        ----------
        None
        """
        assert panproto.ATPROTO_SPEC["schema_theory"] == "ThConstrainedMultiGraph"

    def test_instance_theory(self) -> None:
        """Verify the instance theory is ThWTypeMeta.

        Parameters
        ----------
        None
        """
        assert panproto.ATPROTO_SPEC["instance_theory"] == "ThWTypeMeta"

    def test_edge_rules_count(self) -> None:
        """Verify ATProto has exactly 5 edge rules.

        Parameters
        ----------
        None
        """
        assert len(panproto.ATPROTO_SPEC["edge_rules"]) == 5

    def test_edge_rule_kinds(self) -> None:
        """Verify the edge rule kinds present in ATProto.

        Parameters
        ----------
        None
        """
        kinds = {r["edge_kind"] for r in panproto.ATPROTO_SPEC["edge_rules"]}
        assert kinds == {"record-schema", "prop", "item", "variant", "ref"}

    def test_record_schema_rule(self) -> None:
        """Verify the record-schema edge rule constrains record -> object.

        Parameters
        ----------
        None
        """
        rules = panproto.ATPROTO_SPEC["edge_rules"]
        rule = next(r for r in rules if r["edge_kind"] == "record-schema")
        assert rule["src_kinds"] == ["record"]
        assert rule["tgt_kinds"] == ["object"]

    def test_prop_rule_allows_any_target(self) -> None:
        """Verify the prop edge rule allows any target kind (empty tgt_kinds).

        Parameters
        ----------
        None
        """
        rule = next(r for r in panproto.ATPROTO_SPEC["edge_rules"] if r["edge_kind"] == "prop")
        assert rule["src_kinds"] == ["object"]
        assert rule["tgt_kinds"] == []

    def test_obj_kinds(self) -> None:
        """Verify ATProto object kinds.

        Parameters
        ----------
        None
        """
        assert panproto.ATPROTO_SPEC["obj_kinds"] == ["record", "object"]

    def test_constraint_sorts(self) -> None:
        """Verify ATProto constraint sorts.

        Parameters
        ----------
        None
        """
        expected = ["maxLength", "minLength", "maxGraphemes", "minGraphemes", "format"]
        assert panproto.ATPROTO_SPEC["constraint_sorts"] == expected


# ---------------------------------------------------------------------------
# SQL_SPEC
# ---------------------------------------------------------------------------


class TestSqlSpec:
    """Tests for the SQL built-in protocol specification."""

    def test_name(self) -> None:
        """Verify the protocol name is 'sql'.

        Parameters
        ----------
        None
        """
        assert panproto.SQL_SPEC["name"] == "sql"

    def test_schema_theory(self) -> None:
        """Verify the schema theory is ThConstrainedHypergraph.

        Parameters
        ----------
        None
        """
        assert panproto.SQL_SPEC["schema_theory"] == "ThConstrainedHypergraph"

    def test_instance_theory(self) -> None:
        """Verify the instance theory is ThFunctor.

        Parameters
        ----------
        None
        """
        assert panproto.SQL_SPEC["instance_theory"] == "ThFunctor"

    def test_edge_rules_count(self) -> None:
        """Verify SQL has exactly 3 edge rules.

        Parameters
        ----------
        None
        """
        assert len(panproto.SQL_SPEC["edge_rules"]) == 3

    def test_edge_rule_kinds(self) -> None:
        """Verify the edge rule kinds present in SQL.

        Parameters
        ----------
        None
        """
        kinds = {r["edge_kind"] for r in panproto.SQL_SPEC["edge_rules"]}
        assert kinds == {"column", "fk", "pk"}

    def test_obj_kinds(self) -> None:
        """Verify SQL object kinds.

        Parameters
        ----------
        None
        """
        assert panproto.SQL_SPEC["obj_kinds"] == ["table"]

    def test_constraint_sorts(self) -> None:
        """Verify SQL constraint sorts.

        Parameters
        ----------
        None
        """
        assert panproto.SQL_SPEC["constraint_sorts"] == ["nullable", "unique", "check", "default"]


# ---------------------------------------------------------------------------
# PROTOBUF_SPEC
# ---------------------------------------------------------------------------


class TestProtobufSpec:
    """Tests for the Protobuf built-in protocol specification."""

    def test_name(self) -> None:
        """Verify the protocol name is 'protobuf'.

        Parameters
        ----------
        None
        """
        assert panproto.PROTOBUF_SPEC["name"] == "protobuf"

    def test_schema_theory(self) -> None:
        """Verify the schema theory is ThConstrainedGraph.

        Parameters
        ----------
        None
        """
        assert panproto.PROTOBUF_SPEC["schema_theory"] == "ThConstrainedGraph"

    def test_instance_theory(self) -> None:
        """Verify the instance theory is ThWType.

        Parameters
        ----------
        None
        """
        assert panproto.PROTOBUF_SPEC["instance_theory"] == "ThWType"

    def test_edge_rules_count(self) -> None:
        """Verify Protobuf has exactly 3 edge rules.

        Parameters
        ----------
        None
        """
        assert len(panproto.PROTOBUF_SPEC["edge_rules"]) == 3

    def test_edge_rule_kinds(self) -> None:
        """Verify the edge rule kinds present in Protobuf.

        Parameters
        ----------
        None
        """
        kinds = {r["edge_kind"] for r in panproto.PROTOBUF_SPEC["edge_rules"]}
        assert kinds == {"field", "nested", "value"}

    def test_nested_rule_targets(self) -> None:
        """Verify the nested edge rule allows message and enum targets.

        Parameters
        ----------
        None
        """
        rule = next(r for r in panproto.PROTOBUF_SPEC["edge_rules"] if r["edge_kind"] == "nested")
        assert rule["src_kinds"] == ["message"]
        assert set(rule["tgt_kinds"]) == {"message", "enum"}

    def test_obj_kinds(self) -> None:
        """Verify Protobuf object kinds.

        Parameters
        ----------
        None
        """
        assert panproto.PROTOBUF_SPEC["obj_kinds"] == ["message"]

    def test_constraint_sorts(self) -> None:
        """Verify Protobuf constraint sorts.

        Parameters
        ----------
        None
        """
        expected = ["field-number", "repeated", "optional", "map-key", "map-value"]
        assert panproto.PROTOBUF_SPEC["constraint_sorts"] == expected


# ---------------------------------------------------------------------------
# GRAPHQL_SPEC
# ---------------------------------------------------------------------------


class TestGraphqlSpec:
    """Tests for the GraphQL built-in protocol specification."""

    def test_name(self) -> None:
        """Verify the protocol name is 'graphql'.

        Parameters
        ----------
        None
        """
        assert panproto.GRAPHQL_SPEC["name"] == "graphql"

    def test_schema_theory(self) -> None:
        """Verify the schema theory is ThConstrainedGraph.

        Parameters
        ----------
        None
        """
        assert panproto.GRAPHQL_SPEC["schema_theory"] == "ThConstrainedGraph"

    def test_instance_theory(self) -> None:
        """Verify the instance theory is ThWType.

        Parameters
        ----------
        None
        """
        assert panproto.GRAPHQL_SPEC["instance_theory"] == "ThWType"

    def test_edge_rules_count(self) -> None:
        """Verify GraphQL has exactly 4 edge rules.

        Parameters
        ----------
        None
        """
        assert len(panproto.GRAPHQL_SPEC["edge_rules"]) == 4

    def test_edge_rule_kinds(self) -> None:
        """Verify the edge rule kinds present in GraphQL.

        Parameters
        ----------
        None
        """
        kinds = {r["edge_kind"] for r in panproto.GRAPHQL_SPEC["edge_rules"]}
        assert kinds == {"field", "implements", "member", "value"}

    def test_implements_rule(self) -> None:
        """Verify the implements edge rule constrains type -> interface.

        Parameters
        ----------
        None
        """
        rules = panproto.GRAPHQL_SPEC["edge_rules"]
        rule = next(r for r in rules if r["edge_kind"] == "implements")
        assert rule["src_kinds"] == ["type"]
        assert rule["tgt_kinds"] == ["interface"]

    def test_obj_kinds(self) -> None:
        """Verify GraphQL object kinds.

        Parameters
        ----------
        None
        """
        assert panproto.GRAPHQL_SPEC["obj_kinds"] == ["type", "input"]

    def test_constraint_sorts(self) -> None:
        """Verify GraphQL constraint sorts.

        Parameters
        ----------
        None
        """
        assert panproto.GRAPHQL_SPEC["constraint_sorts"] == ["non-null", "list", "deprecated"]


# ---------------------------------------------------------------------------
# JSON_SCHEMA_SPEC
# ---------------------------------------------------------------------------


class TestJsonSchemaSpec:
    """Tests for the JSON Schema built-in protocol specification."""

    def test_name(self) -> None:
        """Verify the protocol name is 'json-schema'.

        Parameters
        ----------
        None
        """
        assert panproto.JSON_SCHEMA_SPEC["name"] == "json-schema"

    def test_schema_theory(self) -> None:
        """Verify the schema theory is ThConstrainedGraph.

        Parameters
        ----------
        None
        """
        assert panproto.JSON_SCHEMA_SPEC["schema_theory"] == "ThConstrainedGraph"

    def test_instance_theory(self) -> None:
        """Verify the instance theory is ThWType.

        Parameters
        ----------
        None
        """
        assert panproto.JSON_SCHEMA_SPEC["instance_theory"] == "ThWType"

    def test_edge_rules_count(self) -> None:
        """Verify JSON Schema has exactly 3 edge rules.

        Parameters
        ----------
        None
        """
        assert len(panproto.JSON_SCHEMA_SPEC["edge_rules"]) == 3

    def test_edge_rule_kinds(self) -> None:
        """Verify the edge rule kinds present in JSON Schema.

        Parameters
        ----------
        None
        """
        kinds = {r["edge_kind"] for r in panproto.JSON_SCHEMA_SPEC["edge_rules"]}
        assert kinds == {"property", "item", "variant"}

    def test_variant_rule_sources(self) -> None:
        """Verify the variant edge rule allows oneOf and anyOf sources.

        Parameters
        ----------
        None
        """
        rules = panproto.JSON_SCHEMA_SPEC["edge_rules"]
        rule = next(r for r in rules if r["edge_kind"] == "variant")
        assert set(rule["src_kinds"]) == {"oneOf", "anyOf"}
        assert rule["tgt_kinds"] == []

    def test_obj_kinds(self) -> None:
        """Verify JSON Schema object kinds.

        Parameters
        ----------
        None
        """
        assert panproto.JSON_SCHEMA_SPEC["obj_kinds"] == ["object"]

    def test_constraint_sorts(self) -> None:
        """Verify JSON Schema constraint sorts.

        Parameters
        ----------
        None
        """
        expected = [
            "minLength",
            "maxLength",
            "minimum",
            "maximum",
            "pattern",
            "format",
            "required",
        ]
        assert panproto.JSON_SCHEMA_SPEC["constraint_sorts"] == expected


# ---------------------------------------------------------------------------
# BUILTIN_PROTOCOLS registry
# ---------------------------------------------------------------------------


class TestBuiltinProtocols:
    """Tests for the BUILTIN_PROTOCOLS registry mapping."""

    def test_contains_all_five(self) -> None:
        """Verify all five protocol names are present in the registry.

        Parameters
        ----------
        None
        """
        expected_keys = {"atproto", "sql", "protobuf", "graphql", "json-schema"}
        assert set(panproto.BUILTIN_PROTOCOLS.keys()) == expected_keys

    def test_values_are_protocol_specs(self) -> None:
        """Verify each value is a ProtocolSpec dict with required keys.

        Parameters
        ----------
        None
        """
        required_keys = {
            "name",
            "schema_theory",
            "instance_theory",
            "edge_rules",
            "obj_kinds",
            "constraint_sorts",
        }
        for name, spec in panproto.BUILTIN_PROTOCOLS.items():
            assert required_keys.issubset(spec.keys()), f"Missing keys in {name}"

    def test_registry_values_match_module_constants(self) -> None:
        """Verify registry entries are the same objects as the module-level constants.

        Parameters
        ----------
        None
        """
        assert panproto.BUILTIN_PROTOCOLS["atproto"] is panproto.ATPROTO_SPEC
        assert panproto.BUILTIN_PROTOCOLS["sql"] is panproto.SQL_SPEC
        assert panproto.BUILTIN_PROTOCOLS["protobuf"] is panproto.PROTOBUF_SPEC
        assert panproto.BUILTIN_PROTOCOLS["graphql"] is panproto.GRAPHQL_SPEC
        assert panproto.BUILTIN_PROTOCOLS["json-schema"] is panproto.JSON_SCHEMA_SPEC

    def test_name_field_matches_key(self) -> None:
        """Verify each spec's name field matches its registry key.

        Parameters
        ----------
        None
        """
        for key, spec in panproto.BUILTIN_PROTOCOLS.items():
            assert spec["name"] == key, f"Key {key!r} does not match spec name {spec['name']!r}"

    def test_all_edge_rules_have_required_fields(self) -> None:
        """Verify every edge rule across all protocols has edge_kind, src_kinds, tgt_kinds.

        Parameters
        ----------
        None
        """
        for name, spec in panproto.BUILTIN_PROTOCOLS.items():
            for i, rule in enumerate(spec["edge_rules"]):
                assert "edge_kind" in rule, f"{name} rule {i} missing edge_kind"
                assert "src_kinds" in rule, f"{name} rule {i} missing src_kinds"
                assert "tgt_kinds" in rule, f"{name} rule {i} missing tgt_kinds"

    def test_all_obj_kinds_are_nonempty(self) -> None:
        """Verify every protocol has at least one object kind.

        Parameters
        ----------
        None
        """
        for name, spec in panproto.BUILTIN_PROTOCOLS.items():
            assert len(spec["obj_kinds"]) > 0, f"{name} has empty obj_kinds"

    def test_all_constraint_sorts_are_nonempty(self) -> None:
        """Verify every protocol has at least one constraint sort.

        Parameters
        ----------
        None
        """
        for name, spec in panproto.BUILTIN_PROTOCOLS.items():
            assert len(spec["constraint_sorts"]) > 0, f"{name} has empty constraint_sorts"

    def test_unique_theories_per_protocol(self) -> None:
        """Verify that distinct protocols use distinct theory combinations where expected.

        Parameters
        ----------
        None

        Notes
        -----
        ATProto uses ThConstrainedMultiGraph/ThWTypeMeta.
        SQL uses ThConstrainedHypergraph/ThFunctor.
        Protobuf, GraphQL, and JSON Schema share ThConstrainedGraph/ThWType.
        """
        theories = {
            name: (spec["schema_theory"], spec["instance_theory"])
            for name, spec in panproto.BUILTIN_PROTOCOLS.items()
        }
        # ATProto is unique
        assert theories["atproto"] != theories["sql"]
        assert theories["atproto"] != theories["protobuf"]
        # SQL is unique
        assert theories["sql"] != theories["protobuf"]
        # Protobuf, GraphQL, JSON Schema share theories
        assert theories["protobuf"] == theories["graphql"]
        assert theories["protobuf"] == theories["json-schema"]

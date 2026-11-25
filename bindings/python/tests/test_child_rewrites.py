"""Lenses that rewrite a field held by a child of its record's node.

The cases are shared with the Rust suite
(``tests/integration/tests/child_rewrites.rs``): each has a source record,
the view ``get`` shows of it, and for each edited view the source ``put``
gives, or that it refuses.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import panproto
import pytest

ROOT = "com.example.note:body"
CASES: list[dict[str, Any]] = json.loads(
    (
        Path(__file__).parents[3] / "tests" / "fixtures" / "child_rewrites" / "cases.json"
    ).read_text()
)

pytestmark = pytest.mark.parametrize("case", CASES, ids=lambda case: case["name"])


def _expr(source: str) -> dict[str, Any]:
    return panproto.parse_expr(source).to_dict()


def _transform(step: dict[str, Any]) -> dict[str, Any]:
    inverse = _expr(step["from"]) if "from" in step else None
    written = {
        "expr": _expr(step["to"]) if "to" in step else None,
        "inverse": inverse,
        "coercion_class": "Projection" if inverse is None else "Iso",
    }
    if "rewrite" in step:
        return {"ApplyExpr": {"key": step["rewrite"], **written}}
    if "compute" in step:
        return {"ComputeField": {"target_key": step["compute"], **written}}
    if "rename" in step:
        return {"RenameField": {"old_key": step["rename"], "new_key": step["as"]}}
    if "drop" in step:
        return {"DropField": {"key": step["drop"]}}
    if "add" in step:
        return {"AddField": {"key": step["add"], "value": {"Str": step["value"]}}}
    if "keep" in step:
        return {"KeepFields": {"keys": step["keep"]}}
    return {
        "Case": {
            "branches": [
                {
                    "predicate": _expr(predicate),
                    "transforms": [_transform(inner) for inner in branch],
                }
                for predicate, branch in step["case"]
            ]
        }
    }


def _lens(case: dict[str, Any]) -> tuple[panproto.Schema, panproto.Lens]:
    lexicon = {
        "lexicon": 1,
        "id": "com.example.note",
        "defs": {
            "main": {
                "type": "record",
                "key": "any",
                "record": {"type": "object", "properties": case["properties"]},
            }
        },
    }
    schema = panproto.parse_schema_bundle("atproto", [lexicon])
    document = {
        "id": "rewrites",
        "source": "s",
        "target": "t",
        "steps": [{"apply_expr": {"field": "n", "expr": "n", "coercion": "projection"}}],
    }
    chain = json.loads(
        panproto.ProtolensChain.from_dsl_json(json.dumps(document), case["at"]).to_json()
    )
    transforms = {case["at"]: [_transform(step) for step in case["transforms"]]}
    chain["field_transforms"] = transforms
    chain["stages"][0]["field_transforms"] = transforms
    lens = panproto.ProtolensChain.from_json(json.dumps(chain)).instantiate(
        schema, panproto.get_builtin_protocol("atproto")
    )
    return schema, lens


def _read(schema: panproto.Schema, record: Any) -> panproto.Instance:
    return panproto.Instance.from_json(schema, ROOT, json.dumps(record))


def test_an_unedited_view_puts_back_the_source(case: dict[str, Any]) -> None:
    schema, lens = _lens(case)
    source = _read(schema, case["source"])
    view, complement = lens.get(source)

    assert json.loads(view.to_json()) == case["view"]
    assert json.loads(lens.put(view, complement).to_json()) == case["source"]
    lens.check_get_put(source)
    lens.check_put_get(source)


def test_an_edited_view_puts_back_the_expected_source(case: dict[str, Any]) -> None:
    schema, lens = _lens(case)
    _, complement = lens.get(_read(schema, case["source"]))

    for put in case["puts"]:
        if put["source"] == "refused":
            with pytest.raises(panproto.LensError):
                lens.put_json(json.dumps(put["view"]), complement)
            with pytest.raises(panproto.LensError):
                lens.put(lens.align_view(_read(schema, put["view"]), complement), complement)
            continue
        aligned = lens.align_view(_read(schema, put["view"]), complement)
        for restored in (
            json.loads(lens.put_json(json.dumps(put["view"]), complement)),
            json.loads(lens.put(aligned, complement).to_json()),
        ):
            assert restored == put["source"], put["view"]
            again, _ = lens.get(_read(schema, restored))
            assert json.loads(again.to_json()) == put["view"]


def test_an_edit_undone_restores_the_source(case: dict[str, Any]) -> None:
    schema, lens = _lens(case)
    _, complement = lens.get(_read(schema, case["source"]))

    for away in case["puts"] if case["undo"] else []:
        if away["source"] == "refused" or away["view"].get("t") != case["view"].get("t"):
            continue
        there = lens.put_json(json.dumps(away["view"]), complement)
        _, after = lens.get(_read(schema, json.loads(there)))
        assert json.loads(lens.put_json(json.dumps(case["view"]), after)) == case["source"]


def test_the_complement_is_recorded_as_before_where_it_suffices(case: dict[str, Any]) -> None:
    schema, lens = _lens(case)
    _, complement = lens.get(_read(schema, case["source"]))

    recorded = json.loads(json.dumps(complement.to_dict()))
    same = recorded == case["complement_0_78_0"]
    assert same == case["same_complement"]

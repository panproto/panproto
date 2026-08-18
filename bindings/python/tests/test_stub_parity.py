"""The shipped type stub must describe the extension it ships beside.

`py.typed` sits next to `_native.pyi`, so a type checker treats that stub
as the whole truth about `panproto._native` and never consults the
extension module. That makes the stub authoritative in both directions,
and wrong in two different ways:

- A symbol the extension exports and the stub omits is an "unknown import
  symbol" error at every downstream call site, on code that runs fine.
- A symbol the stub declares and the extension lacks type-checks clean
  and raises `AttributeError` at run time, which is the worse direction:
  the checker certifies code that cannot work.

These tests walk the stub with `ast` and compare it against
`dir(panproto._native)` both ways, so neither drift survives a test run.
"""

from __future__ import annotations

import ast
import inspect
from dataclasses import dataclass
from pathlib import Path

import panproto
from panproto import _native as native

_STUB = Path(panproto.__file__).with_name("_native.pyi")


@dataclass(frozen=True)
class _Signature:
    """A parameter list reduced to what both sides can state."""

    positional: list[str]
    keyword_only: list[str]
    var_positional: bool
    var_keyword: bool


def _stub_tree() -> ast.Module:
    return ast.parse(_STUB.read_text(encoding="utf-8"), filename=str(_STUB))


def _is_public(name: str) -> bool:
    return not name.startswith("_")


def _declared_top_level() -> set[str]:
    """Every name the stub declares at module scope."""
    names: set[str] = set()
    for node in _stub_tree().body:
        match node:
            case ast.FunctionDef() | ast.AsyncFunctionDef() | ast.ClassDef():
                names.add(node.name)
            case ast.AnnAssign(target=ast.Name(id=name)):
                names.add(name)
            case ast.Assign(targets=targets):
                names.update(t.id for t in targets if isinstance(t, ast.Name))
            case _:
                pass
    return {n for n in names if _is_public(n)}


def _declared_members() -> dict[str, set[str]]:
    """Every member each stub class declares, inherited members included.

    A stub class body may be empty because everything it offers comes from
    its stub base, as `VcsRepository(Repository)` does, so the bases have
    to be followed or every inherited method reads as undeclared.
    """
    own: dict[str, set[str]] = {}
    bases: dict[str, list[str]] = {}
    for node in _stub_tree().body:
        if not isinstance(node, ast.ClassDef):
            continue
        found: set[str] = set()
        for body in node.body:
            match body:
                case ast.FunctionDef(name=name) | ast.AsyncFunctionDef(name=name):
                    found.add(name)
                case ast.AnnAssign(target=ast.Name(id=name)):
                    found.add(name)
                case _:
                    pass
        own[node.name] = {m for m in found if _is_public(m)}
        bases[node.name] = [b.id for b in node.bases if isinstance(b, ast.Name)]

    def resolve(name: str, seen: frozenset[str]) -> set[str]:
        if name in seen or name not in own:
            return set()
        inherited = seen | {name}
        return own[name].union(*(resolve(base, inherited) for base in bases[name]), set())

    return {name: resolve(name, frozenset()) for name in own}


def _runtime_exports() -> set[str]:
    return {name for name in dir(native) if _is_public(name)}


def test_every_runtime_export_is_declared() -> None:
    """No downstream call site should be an error on code that runs.

    A name here is one the extension exports and the stub omits, so
    `from panproto import <name>` is flagged as an unknown import symbol.
    """
    undeclared = sorted(_runtime_exports() - _declared_top_level())
    assert not undeclared, (
        f"{len(undeclared)} symbol(s) exported by panproto._native are missing "
        f"from _native.pyi, so every call site that imports them is a type "
        f"error: {undeclared}"
    )


def test_every_declared_name_exists_at_runtime() -> None:
    """The stub must not certify a name that raises `AttributeError`."""
    phantom = sorted(_declared_top_level() - _runtime_exports())
    assert not phantom, (
        f"{len(phantom)} name(s) declared in _native.pyi do not exist on "
        f"panproto._native, so a type checker accepts code that raises "
        f"AttributeError: {phantom}"
    )


def test_every_declared_class_member_exists_at_runtime() -> None:
    """The same check one level down, on class members.

    This is the direction that certifies broken code: `ProjectBuilder.build`
    and `GitImportResult.to_dict` both type-checked clean while raising
    `AttributeError`.
    """
    phantom: dict[str, list[str]] = {}
    for class_name, declared in _declared_members().items():
        runtime_class = getattr(native, class_name, None)
        if runtime_class is None:
            continue  # covered by test_every_declared_name_exists_at_runtime
        missing = sorted(m for m in declared if not hasattr(runtime_class, m))
        if missing:
            phantom[class_name] = missing
    assert not phantom, (
        f"members declared in _native.pyi that do not exist on the "
        f"corresponding runtime class: {phantom}"
    )


def test_the_stub_covers_every_runtime_class_member() -> None:
    """And the reverse, so a new method on an existing class is not lost.

    Attributes inherited from `object` and `BaseException` are excluded:
    the stub describes what the extension adds, not what every Python
    object or exception already carries.
    """
    inherited = set(dir(object)) | set(dir(BaseException))
    declared = _declared_members()
    undeclared: dict[str, list[str]] = {}
    for class_name in declared:
        runtime_class = getattr(native, class_name, None)
        if runtime_class is None:
            continue
        exposed = {
            name for name in dir(runtime_class) if _is_public(name) and name not in inherited
        }
        missing = sorted(exposed - declared[class_name])
        if missing:
            undeclared[class_name] = missing
    assert not undeclared, (
        f"members exposed by a runtime class that _native.pyi does not "
        f"declare, so calling them is a type error: {undeclared}"
    )


def _declared_dunders() -> dict[str, set[str]]:
    """Every `__dunder__` each stub class declares, bases followed."""
    own: dict[str, set[str]] = {}
    bases: dict[str, list[str]] = {}
    for node in _stub_tree().body:
        if not isinstance(node, ast.ClassDef):
            continue
        own[node.name] = {
            body.name
            for body in node.body
            if isinstance(body, ast.FunctionDef | ast.AsyncFunctionDef)
            and body.name.startswith("__")
        }
        bases[node.name] = [b.id for b in node.bases if isinstance(b, ast.Name)]

    def resolve(name: str, seen: frozenset[str]) -> set[str]:
        if name in seen or name not in own:
            return set()
        inherited = seen | {name}
        return own[name].union(*(resolve(base, inherited) for base in bases[name]), set())

    return {name: resolve(name, frozenset()) for name in own}


def _own_members(runtime_class: type) -> dict[str, object]:
    """The class's own dictionary, typed.

    `vars` hands back a `MappingProxyType` whose values are `Any`, and the
    comprehension that reads it is under `reportUnknown*`, so the narrowing
    happens here once rather than at the call site.
    """
    return dict(vars(runtime_class))


def test_the_stub_declares_every_protocol_method_the_extension_adds() -> None:
    """A `__dunder__` the extension adds and the stub omits is a type error.

    The four tests above compare names through `_is_public`, which drops
    every `__dunder__` before the comparison, so a protocol method the
    extension implements is invisible to all of them. That is not a
    cosmetic gap: `py.typed` makes the stub authoritative, and a class
    whose stub has no `__len__` makes `len(x)` an error on a call that
    returns a number. `Schema`, `Instance` and `ProtolensChain` each
    implement `__len__` in `crates/panproto-py`.

    The rule is "a callable dunder in the class's own `__dict__` that
    neither `object` nor `BaseException` already carries", rather than a
    list of protocol names, so a future `__iter__` or `__getitem__` is
    covered without editing this test. Two exclusions do real work.
    Reading the class's own dictionary rather than `dir` keeps out the
    inherited exception machinery (`__cause__`, `__traceback__`) and the
    class attributes every type has (`__module__`, `__dict__`), none of
    which a stub declares. And a dunder `object` carries is excluded
    because overriding one changes behaviour without changing what type
    checks: `Vertex.__eq__` compares by value where `object.__eq__`
    compares by identity, and both satisfy the same signature. `pyo3`
    fills in all six rich-comparison slots from one `__eq__`, so that
    exclusion is also what keeps `__lt__` off this list.
    """
    from_python = set(dir(object)) | set(dir(BaseException))
    declared = _declared_dunders()
    undeclared: dict[str, list[str]] = {}
    for class_name in declared:
        runtime_class = getattr(native, class_name, None)
        if runtime_class is None:
            continue
        added = {
            name
            for name, member in _own_members(runtime_class).items()
            if name.startswith("__") and callable(member) and name not in from_python
        }
        missing = sorted(added - declared[class_name])
        if missing:
            undeclared[class_name] = missing
    assert not undeclared, (
        f"protocol methods a runtime class implements that _native.pyi does "
        f"not declare, so using them is a type error on code that runs: "
        f"{undeclared}"
    )


def _stub_parameters(fn: ast.FunctionDef, drop_receiver: bool) -> _Signature:
    """The parameter names a stub declaration states, in three groups."""
    args = fn.args
    positional = [p.arg for p in args.posonlyargs] + [p.arg for p in args.args]
    if drop_receiver and positional and positional[0] in {"self", "cls"}:
        positional = positional[1:]
    return _Signature(
        positional=positional,
        keyword_only=[p.arg for p in args.kwonlyargs],
        var_positional=args.vararg is not None,
        var_keyword=args.kwarg is not None,
    )


def _runtime_parameters(obj: object, drop_receiver: bool) -> _Signature | None:
    """The same three groups, read off pyo3's `__text_signature__`.

    `None` when the object exposes no signature at all, which three of the
    extension's types do; those are skipped rather than guessed at.
    """
    try:
        signature = inspect.signature(obj)  # type: ignore[arg-type]
    except (ValueError, TypeError):
        return None
    positional: list[str] = []
    keyword_only: list[str] = []
    var_positional = var_keyword = False
    for name, parameter in signature.parameters.items():
        match parameter.kind:
            case inspect.Parameter.POSITIONAL_ONLY | inspect.Parameter.POSITIONAL_OR_KEYWORD:
                positional.append(name)
            case inspect.Parameter.KEYWORD_ONLY:
                keyword_only.append(name)
            case inspect.Parameter.VAR_POSITIONAL:
                var_positional = True
            case inspect.Parameter.VAR_KEYWORD:
                var_keyword = True
    if drop_receiver and positional and positional[0] in {"self", "cls", "$self", "$cls"}:
        positional = positional[1:]
    return _Signature(
        positional=positional,
        keyword_only=keyword_only,
        var_positional=var_positional,
        var_keyword=var_keyword,
    )


def test_every_stub_signature_matches_the_extension() -> None:
    """Parameter names and kinds, not merely the existence of the name.

    The four tests above compare `dir()`, which certifies a stub whose every
    signature is wrong: a caller writing `repo.log(limit=1)` gets a type error
    on a call that runs, and `parse_expr(text=...)` type-checks and raises
    `TypeError`. Both directions are call-site failures on the same footing as
    a missing symbol, because `py.typed` makes the stub authoritative.

    Return types are outside this check. `__text_signature__` does not carry
    one, so nothing here would have caught `Repository.index` being declared
    `list[...]` where the extension returns a `dict`.
    """
    mismatched: list[str] = []
    for node in _stub_tree().body:
        match node:
            case ast.FunctionDef():
                target = getattr(native, node.name, None)
                if target is None:
                    continue  # covered by test_every_declared_name_exists_at_runtime
                _compare(node.name, node, target, drop_receiver=False, into=mismatched)
            case ast.ClassDef():
                runtime_class = getattr(native, node.name, None)
                if runtime_class is None:
                    continue
                for member in node.body:
                    if not isinstance(member, ast.FunctionDef):
                        continue
                    # `__init__` is declared on the stub class and implemented
                    # by pyo3's `#[new]`, whose signature sits on the type
                    # rather than on the slot wrapper.
                    target = (
                        runtime_class
                        if member.name == "__init__"
                        else getattr(runtime_class, member.name, None)
                    )
                    if target is None:
                        continue
                    _compare(
                        f"{node.name}.{member.name}",
                        member,
                        target,
                        drop_receiver=True,
                        into=mismatched,
                    )
            case _:
                # Imports, assignments and `TypeAlias` statements carry no
                # signature to compare.
                pass
    assert not mismatched, (
        f"{len(mismatched)} stub signature(s) disagree with the extension:\n"
        + "\n".join(mismatched)
    )


def _compare(
    label: str,
    node: ast.FunctionDef,
    target: object,
    *,
    drop_receiver: bool,
    into: list[str],
) -> None:
    runtime = _runtime_parameters(target, drop_receiver)
    if runtime is None:
        return
    stub = _stub_parameters(node, drop_receiver)
    if stub != runtime:
        into.append(f"  {label}\n     stub   : {stub}\n     runtime: {runtime}")

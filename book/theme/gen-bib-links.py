#!/usr/bin/env python3
"""Generate book/theme/bib-links.js from book/src/references.bib.

The output is loaded by bib-linkify.js, which turns each bibliography
entry's title into a hyperlink to the DOI/URL and removes the trailing
bare URL. Run after editing references.bib.
"""
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BIB = ROOT / "src" / "references.bib"
OUT = ROOT / "theme" / "bib-links.js"


def field(body: str, name: str) -> str | None:
    m = re.search(r"\b" + name + r"\s*=\s*\{((?:[^{}]|\{[^{}]*\})*)\}", body, re.IGNORECASE)
    if m:
        return m.group(1).strip()
    m = re.search(r"\b" + name + r'\s*=\s*"([^"]*)"', body, re.IGNORECASE)
    return m.group(1).strip() if m else None


def clean(s: str | None) -> str | None:
    if not s:
        return s
    s = re.sub(r"[{}]", "", s)
    return re.sub(r"\s+", " ", s).strip()


def url_of(body: str) -> str | None:
    doi = field(body, "doi")
    if doi:
        return "https://doi.org/" + doi.strip()
    for f in ("url", "howpublished", "note"):
        v = field(body, f)
        if v:
            m = re.search(r"https?://[^\s,;}]+", v)
            if m:
                return m.group(0).rstrip(".").rstrip(",")
    eprint = field(body, "eprint")
    arxiv = field(body, "archiveprefix")
    if eprint and arxiv and "arxiv" in arxiv.lower():
        return "https://arxiv.org/abs/" + eprint.strip()
    return None


def main() -> None:
    text = BIB.read_text()
    entries = re.findall(r"@\w+\{([^,]+),(.*?)\n\}", text, re.DOTALL)
    out: dict[str, dict[str, str]] = {}
    for key, body in entries:
        key = key.strip()
        title = clean(field(body, "title"))
        url = url_of(body)
        if title and url:
            out[key] = {"title": title, "url": url}
    js = "window.__BIB_LINKS = " + json.dumps(out, ensure_ascii=False, indent=0) + ";\n"
    OUT.write_text(js)
    print(f"Wrote {len(out)} entries to {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()

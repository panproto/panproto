# Specification sources for owner decision 38, fetched 2026-10-14 (13:16 UTC)

| Source | URL | Local text |
|---|---|---|
| Lexicon, "String Formats" | https://atproto.com/specs/lexicon | lexicon.txt |
| Handle, "Handle Identifier Syntax" | https://atproto.com/specs/handle | handle.txt |
| NSID, "NSID Syntax" | https://atproto.com/specs/nsid | nsid.txt |
| TID, "TID Syntax" | https://atproto.com/specs/tid | tid.txt |
| Record Key, "Record Key Syntax" | https://atproto.com/specs/record-key | record-key.txt |
| AT URI, "Restricted AT URI Syntax" | https://atproto.com/specs/at-uri-scheme | at-uri-scheme.txt |
| DID, "DID Identifier Syntax" (for at-identifier, at-uri) | https://atproto.com/specs/did | did.txt |
| Data model, "Link and CID Formats" | https://atproto.com/specs/data-model | data-model.txt |
| DASL CID, "Parsing CIDs" | https://dasl.ing/cid.html | dasl-cid.txt |
| RFC 5646 (BCP 47), Sec 2.1 and 2.1.1 | https://www.rfc-editor.org/rfc/rfc5646.txt | rfc5646.txt |
| atproto-interop-tests README and syntax vectors | https://github.com/bluesky-social/atproto-interop-tests (commit 056e5741bb330757205d6b16db5266fffcae937b) | atproto-interop-tests-README.md; vectors in tests/fixtures/atproto-interop/syntax, byte-identical (cmp) |

Implementations read only to explain why some interop vectors disagree with the specifications (not used as a source of rules):

| File | URL |
|---|---|
| indigo-cid.go | https://raw.githubusercontent.com/bluesky-social/indigo/main/atproto/syntax/cid.go |
| indigo-language.go | https://raw.githubusercontent.com/bluesky-social/indigo/main/atproto/syntax/language.go |
| ts-formats.ts | https://raw.githubusercontent.com/bluesky-social/atproto/main/packages/lexicon/src/validators/formats.ts |
| ts-util.ts | https://raw.githubusercontent.com/bluesky-social/atproto/main/packages/lexicon/src/util.ts |

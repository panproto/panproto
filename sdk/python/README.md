# panproto — Python SDK

Universal schema migration engine for Python 3.13+.

## Installation

```bash
pip install panproto
```

## Quick Start

```python
from panproto import Panproto

with Panproto.load() as pp:
    atproto = pp.protocol("atproto")
    schema = (
        atproto.schema()
        .vertex("post", "record", {"nsid": "app.bsky.feed.post"})
        .vertex("post:body", "object")
        .edge("post", "post:body", "record-schema")
        .build()
    )
```

## License

MIT

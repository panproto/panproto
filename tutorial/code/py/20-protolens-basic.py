from panproto import Panproto

panproto = Panproto.init()

# One-liner conversion
result = panproto.convert(
    {"text": "Hello world", "complete": True},
    from_schema=v1,
    to_schema=v2,
    defaults={"status": "done"},
)
print(result)  # {"text": "Hello world", "status": "done"}

# Reusable protolens chain
with ProtolensChainHandle.auto_generate(v1, v2, panproto._wasm) as chain:
    spec = chain.requirements(v1)
    print(f"Needs defaults: {spec.forward_defaults}")

    with chain.instantiate(v1) as lens:
        view, complement = lens.get(post_data)
        restored = lens.put(view, complement)

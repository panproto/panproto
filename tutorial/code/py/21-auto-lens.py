from panproto import Panproto, LensHandle, ProtolensChainHandle

panproto = Panproto.init()

# Auto-generate
with LensHandle.auto_generate(old_schema, new_schema, panproto._wasm) as lens:
    view, complement = lens.get(old_data)

# Reusable chain
with ProtolensChainHandle.auto_generate(old_schema, new_schema, panproto._wasm) as chain:
    chain_json = chain.to_json()

# Later, different project
with ProtolensChainHandle.from_json(chain_json, panproto._wasm) as chain2:
    with chain2.instantiate(my_schema) as lens2:
        view, complement = lens2.get(my_data)

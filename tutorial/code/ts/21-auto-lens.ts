import { Panproto, LensHandle, ProtolensChainHandle } from '@panproto/core';

const panproto = await Panproto.init();

// Auto-generate a lens between any two schemas
using lens = LensHandle.autoGenerate(oldSchema, newSchema, panproto._wasm);

// The lens works immediately
const { view, complement } = lens.get(oldData);

// Or get the reusable chain for cross-project use
using chain = ProtolensChainHandle.autoGenerate(oldSchema, newSchema, panproto._wasm);

// Save for later — works across schemas with the same structure
const chainJson = chain.toJson();

// Later, in a different project with compatible schemas:
using chain2 = ProtolensChainHandle.fromJson(chainJson, panproto._wasm);
using lens2 = chain2.instantiate(myProjectSchema);

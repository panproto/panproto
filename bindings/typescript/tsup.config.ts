import { defineConfig } from 'tsup';

// We build the published library with tsup (esbuild + tsc for .d.ts) rather
// than Vite's lib mode. Vite's lib mode rewrites
// `new URL('./asset', import.meta.url)` into
// `new URL('./asset', "" + import.meta.url)`, which defeats the AST shape
// downstream bundlers (Vite, Rollup, esbuild, Webpack 5) detect to bundle
// sibling assets. The result was a runtime 404 on the wasm-bindgen glue
// in production Vite consumer builds (panproto/panproto#57).
//
// tsup/esbuild leaves `import.meta.url` references untouched, so the
// shipped `dist/index.{js,cjs}` is bundler-friendly: consumers can call
// `Panproto.init()` and Vite/Rollup/esbuild/Webpack will copy
// `panproto_wasm.js` and `panproto_wasm_bg.wasm` into their output.
export default defineConfig({
  entry: ['src/index.ts'],
  format: ['esm', 'cjs'],
  outDir: 'dist',
  outExtension: ({ format }) => ({
    js: format === 'esm' ? '.js' : '.cjs',
  }),
  // Generate .d.ts (and .d.cts via the cjs build).
  dts: true,
  target: 'es2022',
  platform: 'neutral',
  sourcemap: true,
  splitting: false,
  clean: false,
  treeshake: true,
  shims: false,
  external: [
    '@msgpack/msgpack',
    // node:* built-ins are dynamically imported behind an `IS_NODE`
    // runtime guard in src/wasm.ts; mark them external so non-Node
    // consumers don't try to resolve them and so esbuild doesn't warn.
    /^node:/,
  ],
});

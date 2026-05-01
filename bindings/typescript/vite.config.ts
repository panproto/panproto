import { defineConfig } from 'vite';
import { resolve } from 'node:path';
import dts from 'vite-plugin-dts';

export default defineConfig({
  plugins: [
    dts(),
  ],
  build: {
    lib: {
      entry: resolve(__dirname, 'src/index.ts'),
      name: 'PanprotoCore',
      formats: ['es', 'cjs'],
      fileName: (format) => format === 'es' ? 'index.js' : 'index.cjs',
    },
    rollupOptions: {
      // `node:*` imports are guarded by runtime environment detection in
      // wasm.ts (IS_NODE). Keep them as-is so Node resolves built-ins;
      // browsers never execute the gated code path.
      external: ['@msgpack/msgpack', /^node:/],
    },
    target: 'es2022',
    sourcemap: true,
    minify: false,
  },
  test: {
    globals: true,
    environment: 'node',
    include: ['tests/**/*.test.ts'],
  },
});

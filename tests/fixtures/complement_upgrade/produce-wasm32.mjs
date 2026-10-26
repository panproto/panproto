// Produce `wasm32.named.msgpack` for every case in this directory by running
// the case's lens through a wasm32 build of panproto-wasm.
//
// Usage (from the repo root):
//
//   wasm-pack build crates/panproto-wasm --target nodejs --dev \
//     --out-dir <pkg> --out-name panproto_wasm
//   node tests/fixtures/complement_upgrade/produce-wasm32.mjs <pkg> \
//     bindings/typescript/node_modules/@msgpack/msgpack
//
// Each case directory supplies the protocol, the build operations of both
// schemas, the migration and the source instance as named msgpack. The
// script calls `define_protocol`, `build_schema` (twice),
// `compile_migration` and `get_record`, and writes the complement bytes that
// `get_record` returns, unchanged.

import { createRequire } from "node:module";
import { readdirSync, readFileSync, statSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const [pkgDir, msgpackDir] = process.argv.slice(2);
if (!pkgDir || !msgpackDir) {
  console.error("usage: produce-wasm32.mjs <wasm-pack nodejs pkg dir> <@msgpack/msgpack dir>");
  process.exit(2);
}

const require = createRequire(import.meta.url);
const wasm = require(resolve(pkgDir, "panproto_wasm.js"));
const { decode } = await import(resolve(msgpackDir, "dist.esm", "index.mjs"));

const here = dirname(fileURLToPath(import.meta.url));
for (const name of readdirSync(here).sort()) {
  const dir = join(here, name);
  if (!statSync(dir).isDirectory() || !existsSync(join(dir, "instance.msgpack"))) {
    continue;
  }
  const read = (file) => new Uint8Array(readFileSync(join(dir, file)));
  const proto = wasm.define_protocol(read("protocol.msgpack"));
  const src = wasm.build_schema(proto, read("src_ops.msgpack"));
  const tgt = wasm.build_schema(proto, read("tgt_ops.msgpack"));
  const migration = wasm.compile_migration(src, tgt, read("migration.msgpack"));
  const result = decode(wasm.get_record(migration, read("instance.msgpack")));
  const complement = Uint8Array.from(result.complement);
  writeFileSync(join(dir, "wasm32.named.msgpack"), complement);
  console.log(`${name}: ${complement.length} bytes`);
}

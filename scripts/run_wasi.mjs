// Runs a program built for wasm32-wasip1 (such as pd-core's consistency_dump
// example) under Node's WASI, with the repository and an output folder made
// visible to it as /repo and /out.
//
//     node scripts/run_wasi.mjs <program.wasm> <repository> <out>
import { readFile } from 'node:fs/promises';
import { mkdirSync } from 'node:fs';
import { WASI } from 'node:wasi';

const [wasmPath, repo, out] = process.argv.slice(2);
if (!out) {
  console.error('usage: node scripts/run_wasi.mjs <program.wasm> <repository> <out>');
  process.exit(2);
}
mkdirSync(out, { recursive: true });
const wasi = new WASI({
  version: 'preview1',
  args: ['program', '/repo', '/out'],
  preopens: { '/repo': repo, '/out': out },
});
const module = await WebAssembly.compile(await readFile(wasmPath));
const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
wasi.start(instance);

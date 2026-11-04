import { spawnSync } from 'node:child_process';
import { mkdirSync, copyFileSync } from 'node:fs';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '..');
const target = 'x86_64-pc-windows-msvc';
const cross = process.platform !== 'win32';
const output = process.env.CARGO_TARGET_DIR || path.join(root, 'src-tauri/target');
const env = { ...process.env, SHERPA_ONNX_LIB_DIR: path.join(root, 'src-tauri/target/windows-runtime/lib') };

function run(command, args, extra = {}) {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', env: { ...env, ...extra } });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} exited with ${result.status}`);
}
const cargo = args => [...(cross ? ['xwin'] : []), ...args, '--manifest-path', 'src-tauri/Cargo.toml', '--target', target];
run(process.execPath, ['scripts/prepare-windows-runtime.mjs']);
run('cargo', cargo(['build', '--release', '-p', 'humla-windows']));
const binaries = path.join(root, 'src-tauri/binaries');
mkdirSync(binaries, { recursive: true });
for (const name of ['audio-capture', 'speaker-diarize']) {
  copyFileSync(path.join(output, target, 'release', `${name}.exe`), path.join(binaries, `${name}-${target}.exe`));
}
run('cargo', cargo(['build', '--release', '-p', 'humla', '--bin', 'humla-mcp']), {
  TAURI_CONFIG: JSON.stringify({ bundle: { externalBin: [] } }),
});
copyFileSync(path.join(output, target, 'release/humla-mcp.exe'), path.join(binaries, `humla-mcp-${target}.exe`));
run(process.execPath, ['node_modules/@tauri-apps/cli/tauri.js', 'build', '--target', target,
  ...(cross ? ['--runner', 'cargo-xwin'] : []),
  '--config', JSON.stringify({ bundle: { createUpdaterArtifacts: Boolean(process.env.TAURI_SIGNING_PRIVATE_KEY) } }),
]);

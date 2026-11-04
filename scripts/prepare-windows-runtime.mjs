import { readFile, writeFile, mkdir, readdir, copyFile, access } from 'node:fs/promises';
import { createWriteStream } from 'node:fs';
import { pipeline } from 'node:stream/promises';
import { spawnSync } from 'node:child_process';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '..');
const cache = path.join(root, 'src-tauri/target/windows-runtime');
const config = JSON.parse(await readFile(path.join(import.meta.dirname, 'windows-runtime.json'), 'utf8'));
const run = (program, args, options = {}) => {
  const result = spawnSync(program, args, { stdio: 'inherit', ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${program} exited with ${result.status}`);
};
const exists = file => access(file).then(() => true, () => false);
async function extract(name, url) {
  const directory = path.join(cache, name);
  if (await exists(path.join(directory, '.complete'))) return directory;
  await mkdir(directory, { recursive: true });
  const archive = path.join(cache, `${name}.zip`);
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  await pipeline(response.body, createWriteStream(archive));
  run('cmake', ['-E', 'tar', 'xf', archive], { cwd: directory });
  await writeFile(path.join(directory, '.complete'), '');
  return directory;
}

await mkdir(cache, { recursive: true });
const [sourceArchive, ort, dml, vclibs] = await Promise.all([
  extract('source', config.source),
  extract('onnxruntime', config.onnxruntime),
  extract('directml', config.directml),
  extract('vclibs', config.vclibs),
]);
const source = path.join(sourceArchive, config.sourceDirectory);
const session = path.join(source, 'sherpa-onnx/csrc/session.cc');
let code = await readFile(session, 'utf8');
const directml = await readFile(path.join(root, 'src-tauri/crates/windows/native/directml-session.inc'), 'utf8');
const fallback = /      int32_t device_id = 0;\n      OrtStatus \*status =\n          OrtSessionOptionsAppendExecutionProvider_DML\(sess_opts, device_id\);[\s\S]*?(?=^#else)/m;
if (fallback.test(code)) {
  code = code.replace(fallback, directml);
  await writeFile(session, code);
}
if (!code.includes(directml)) throw new Error('DirectML session setup changed');
const native = path.join(ort, 'runtimes/win-x64/native');
const build = path.join(cache, 'build');
const output = path.join(cache, 'lib');
const options = Object.entries(config.cmake).map(([key, value]) => `-D${key}=${value}`);
if (process.env.WIN_CMAKE_TOOLCHAIN_FILE) options.push(`-DCMAKE_TOOLCHAIN_FILE=${process.env.WIN_CMAKE_TOOLCHAIN_FILE}`);
const env = {
  ...process.env,
  SHERPA_ONNXRUNTIME_INCLUDE_DIR: path.join(ort, 'build/native/include'),
  SHERPA_ONNXRUNTIME_LIB_DIR: native,
};
run('cmake', ['-S', source, '-B', build, '-G', 'Ninja', ...options,
  `-DFETCHCONTENT_SOURCE_DIR_DIRECTML=${dml}`,
  `-Dlocation_onnxruntime_lib2=${path.join(native, 'onnxruntime.lib')}`,
  `-DCMAKE_INSTALL_PREFIX=${path.join(cache, 'installed')}`], { env });
run('cmake', ['--build', build, '--target', 'sherpa-onnx-c-api', '--parallel', process.env.CMAKE_BUILD_PARALLEL_LEVEL || '3'], { env });
await mkdir(output, { recursive: true });
async function collect(directory, names) {
  for (const item of await readdir(directory, { withFileTypes: true })) {
    const file = path.join(directory, item.name);
    if (item.isDirectory()) await collect(file, names);
    else if (names.test(item.name)) await copyFile(file, path.join(output, item.name));
  }
}
await collect(build, /^(sherpa-onnx-(c-api|core)|kaldi-decoder-core|sherpa-onnx-kaldifst-core)\.(dll|lib)$/i);
await collect(native, /^(onnxruntime(?:_providers_shared)?|DirectML)\.(dll|lib)$/i);
await collect(vclibs, /^(msvcp140(?:_\d)?|vcruntime140(?:_\d)?)\.dll$/i);
await copyFile(path.join(dml, 'bin/x64-win/DirectML.dll'), path.join(output, 'DirectML.dll'));
await copyFile(path.join(source, 'LICENSE'), path.join(output, 'sherpa-onnx-LICENSE'));
for (const [directory, name] of [[ort, 'onnxruntime'], [dml, 'DirectML']]) {
  for (const item of await readdir(directory)) {
    if (/license/i.test(item)) await copyFile(path.join(directory, item), path.join(output, `${name}-${item}`));
  }
}
console.log(output);

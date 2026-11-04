import { readFile, writeFile, readdir, mkdir } from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const { GITHUB_REPOSITORY: repository, RELEASE_TAG: tag, RELEASE_BASE_URL: base } = process.env;
if (!repository || !tag || !base) throw new Error('Missing release destination');
const run = args => {
  const result = spawnSync('gh', args, { stdio: 'inherit' });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`gh exited with ${result.status}`);
};
const directory = path.resolve('src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis');
const installers = (await readdir(directory)).filter(name => name.endsWith('-setup.exe'));
if (installers.length !== 1) throw new Error('Expected one Windows installer');
const name = installers[0];
const signature = (await readFile(path.join(directory, `${name}.sig`), 'utf8')).trim();
const manifest = path.resolve('src-tauri/target/windows-update/latest.json');
await mkdir(path.dirname(manifest), { recursive: true });
run(['release', 'download', tag, '--repo', repository, '--pattern', 'latest.json', '--output', manifest, '--clobber']);
const update = JSON.parse(await readFile(manifest, 'utf8'));
const version = JSON.parse(await readFile('package.json', 'utf8')).version;
if (update.version !== version || tag !== `v${version}`) throw new Error('Release version mismatch');
update.platforms['windows-x86_64'] = { signature, url: `${base}/${tag}/${name}` };
await writeFile(manifest, `${JSON.stringify(update, null, 2)}\n`);
run(['release', 'upload', tag, '--repo', repository, path.join(directory, name), path.join(directory, `${name}.sig`), '--clobber']);
run(['release', 'upload', tag, '--repo', repository, manifest, '--clobber']);

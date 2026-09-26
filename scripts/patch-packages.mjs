// Applies patches/*.patch that target node_modules to the installed packages.
// Runs from npm's postinstall; idempotent, so reinstalls and repeated runs are safe.
import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = fileURLToPath(new URL('../', import.meta.url));
const patches = readdirSync(path.join(repo, 'patches'))
  .filter(name => name.endsWith('.patch'))
  .map(name => path.join(repo, 'patches', name))
  .filter(file => /^\+\+\+ b\/node_modules\//m.test(readFileSync(file, 'utf8')));

for (const patch of patches) {
  const apply = (...flags) =>
    execFileSync('git', ['apply', ...flags, patch], { cwd: repo, stdio: ['ignore', 'ignore', 'pipe'] });
  try {
    apply('--reverse', '--check');
    console.log(`  already applied: ${path.basename(patch)}`);
    continue;
  } catch {}
  apply('--check');
  apply();
  console.log(`  applied: ${path.basename(patch)}`);
}

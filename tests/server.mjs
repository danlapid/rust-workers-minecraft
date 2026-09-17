import assert from 'node:assert/strict';
import net from 'node:net';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, appendFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { waitFor } from './client.mjs';

const repo = fileURLToPath(new URL('../', import.meta.url));

export async function testServer({ seed = '1789195700909824175' } = {}) {
  await mkdir(path.join(repo, '.data/probes'), { recursive: true });
  const probe = await mkdtemp(path.join(repo, '.data/probes/workers.'));
  const persistence = path.join(probe, 'state');
  console.log(`Workers probe state: ${probe}`);
  for (const port of [25565, 8787]) {
    const listener = net.createServer();
    await new Promise((resolve, reject) => {
      listener.once('error', reject);
      listener.listen(port, '127.0.0.1', resolve);
    });
    await new Promise(resolve => listener.close(resolve));
  }

  let phase = 0;
  async function startWorker() {
    const log = path.join(probe, `${++phase}-minecraft.log`);
    const child = spawn(process.execPath, [
      path.join(repo, 'node_modules/wrangler/bin/wrangler.js'), 'dev', '--local',
      '--config', path.join(repo, 'wrangler.jsonc'),
      '--persist-to', persistence, '--var', 'WORLD_NAME:integration',
      // Known terrain with a stable editable spawn block; random water spawns
      // cannot exercise the integration test's shared block-edit assertions.
      '--var', `WORLD_SEED:${seed}`,
    ], {
      cwd: repo, detached: true, stdio: ['ignore', 'pipe', 'pipe'],
      env: { ...process.env, WRANGLER_SEND_METRICS: 'false', CI: 'true', NO_COLOR: '1' },
    });
    let exited = false, failure, logWrites = Promise.resolve();
    const closed = new Promise(resolve => child.once('close', resolve));
    child.once('error', error => { failure = error; });
    child.once('exit', () => { exited = true; });
    for (const stream of [child.stdout, child.stderr]) {
      let tail = '';
      stream.on('data', data => {
        logWrites = logWrites.then(() => appendFile(log, data));
        const output = (tail + data.toString()).replace(/\x1b\[[0-9;]*m/g, '');
        if (/RUST PANIC:|Fatal uncaught|runtime crashed unexpectedly|(?:^|\n)\s*ERROR\b/.test(output)) {
          failure = new Error(`Runtime failure; see ${log}`);
        }
        tail = output.slice(-128);
      });
    }
    async function stop() {
      // A dedicated process group contains only this probe's Wrangler/workerd.
      const kill = signal => {
        try { process.kill(-child.pid, signal); }
        catch (error) { if (error.code !== 'ESRCH') throw error; }
      };
      kill('SIGTERM');
      const force = setTimeout(() => kill('SIGKILL'), 3000);
      await closed;
      clearTimeout(force);
      await logWrites;
    }
    try {
      await waitFor(async () => {
        if (failure) throw failure;
        if (exited) throw new Error(`Wrangler exited; see ${log}`);
        try { return (await fetch('http://127.0.0.1:8787/health', { signal: AbortSignal.timeout(2000) })).ok; }
        catch { return false; }
      }, `Wrangler (${log})`);
    } catch (error) { await stop(); throw error; }
    return {
      stop,
      healthy() { if (failure) throw failure; assert.ok(!exited, `Wrangler exited; see ${log}`); },
    };
  }

  async function request(route = '/', options) {
    const response = await fetch(`http://127.0.0.1:8787${route}`, { ...options, signal: AbortSignal.timeout(60_000) });
    const text = await response.text();
    assert.ok(response.ok, `HTTP ${response.status}: ${text.slice(0,2000)}`);
    return JSON.parse(text);
  }

  async function checkpoint(worker) {
    return waitFor(async () => {
      worker.healthy();
      let state;
      try { state = await request('/'); }
      catch (error) {
        // workerd can cancel a concurrent status RPC as the TCP event closes.
        if (error.message.startsWith('HTTP 500: Error: Network connection lost.')) return false;
        throw error;
      }
      if (state.failure) throw new Error(state.failure);
      return state.phase === 'idle' && state.saved_at && state.connections === 0 && state;
    }, 'world checkpoint');
  }

  return { startWorker, request, checkpoint, probe, seed };
}

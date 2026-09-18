import { MinecraftWorld } from '../../worker/world';

export class StartupFailure extends MinecraftWorld {
  loadRuntime() { throw new Error('Expected startup failure'); }

  async verify() {
    const attempt = this.getRuntime();
    const starting = this.status();
    try { await attempt; } catch {}
    const failed = this.status();
    try { await this.getRuntime(); } catch {}
    return { starting, failed, repeated: this.status() };
  }
}

export class CheckpointFailure extends MinecraftWorld {
  loadRuntime() {
    return Promise.resolve({
      connect: async () => {}, stop: async () => {},
      save: () => new Promise(resolve => { this.finishSave = resolve; }),
      status: () => {
        if (this.failStatus) throw new Error('Expected failure during checkpoint');
        return { players: 0, ticks: 1, wasm_memory_bytes: 0 };
      },
    });
  }

  async verify() {
    await this.getRuntime();
    const checkpoint = this.checkpoint(false);
    await Promise.resolve();
    this.failStatus = true;
    const failed = this.status();
    this.finishSave();
    let rejected = false;
    try { await checkpoint; } catch { rejected = true; }
    return { failed, after: this.status(), rejected };
  }
}

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

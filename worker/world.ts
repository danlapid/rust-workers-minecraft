import { DurableObject } from 'cloudflare:workers';
import { startPumpkin } from './pumpkin';

type Runtime = Awaited<ReturnType<typeof startPumpkin>>;
type State =
  | { phase: 'idle' }
  | { phase: 'starting'; pending: Promise<Runtime> }
  | { phase: 'running'; runtime: Runtime }
  | { phase: 'stopping'; pending: Promise<void> }
  | { phase: 'failed'; error: string };

export class MinecraftWorld extends DurableObject<Env> {
  private state: State = { phase: 'idle' };
  private connections = 0;
  private startupMilliseconds?: number;
  private checkpointedAt?: string;

  protected loadRuntime(): Promise<Runtime> {
    return startPumpkin(this.ctx.storage);
  }

  private fail(error: unknown): void {
    if (this.state.phase !== 'failed') {
      this.state = { phase: 'failed', error: error instanceof Error ? error.message : String(error) };
    }
  }

  protected async getRuntime(): Promise<Runtime> {
    const state = this.state;
    switch (state.phase) {
      case 'running': return state.runtime;
      case 'starting': return state.pending;
      case 'failed': throw new Error(state.error);
      case 'stopping':
        await state.pending;
        return this.getRuntime();
      case 'idle': {
        const started = Date.now();
        const pending = Promise.resolve().then(() => this.loadRuntime()).then(runtime => {
          this.startupMilliseconds = Date.now() - started;
          this.state = { phase: 'running', runtime };
          return runtime;
        }).catch(error => { this.fail(error); throw error; });
        this.state = { phase: 'starting', pending };
        return pending;
      }
    }
  }

  async connect(socket: Socket): Promise<void> {
    void socket.closed.catch(() => undefined);
    if (this.state.phase === 'stopping') await this.state.pending;
    this.connections++;
    try {
      await (await this.getRuntime()).connect(socket);
    } finally {
      this.connections--;
      await socket.close().catch(() => undefined);
      if (this.connections === 0) await this.checkpoint();
    }
  }

  private async checkpoint(): Promise<void> {
    const state = this.state;
    if (state.phase === 'stopping') return state.pending;
    if (state.phase !== 'running') return;
    const pending = (async () => {
      await state.runtime.stop();
      await this.ctx.storage.sync();
      this.checkpointedAt = new Date().toISOString();
      this.state = { phase: 'idle' };
    })().catch(error => { this.fail(error); throw error; });
    this.state = { phase: 'stopping', pending };
    return pending;
  }

  status() {
    let server: ReturnType<Runtime['status']> | null = null;
    if (this.state.phase === 'running') {
      try { server = this.state.runtime.status(); }
      catch (error) { this.fail(error); }
    }
    return {
      phase: this.state.phase,
      connections: this.connections,
      failure: this.state.phase === 'failed' ? this.state.error : null,
      startup_ms: this.startupMilliseconds ?? null,
      checkpointed_at: this.checkpointedAt ?? null,
      server,
    };
  }
}

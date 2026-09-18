import { DurableObject } from 'cloudflare:workers';
import { startPumpkin } from './pumpkin';
import { readSettings, type ServerSettings } from './config';
import { MIN_PROTOCOL, MAX_PROTOCOL, VERSION_NAME } from './minecraft-status';

type Runtime = Awaited<ReturnType<typeof startPumpkin>>;
type State =
  | { phase: 'idle' }
  | { phase: 'starting'; pending: Promise<Runtime> }
  | { phase: 'running'; runtime: Runtime }
  | { phase: 'saving'; runtime: Runtime; pending: Promise<void> }
  | { phase: 'stopping'; pending: Promise<void> }
  | { phase: 'failed'; error: string };

export class MinecraftWorld extends DurableObject<Env> {
  private state: State = { phase: 'idle' };
  private connections = 0;
  private startupMilliseconds?: number;
  private checkpointedAt?: string;
  private starts = 0;
  private idleTimer?: ReturnType<typeof setTimeout>;
  private idleDeadline?: number;

  protected loadRuntime(): Promise<Runtime> {
    return startPumpkin(this.ctx.storage, readSettings(this.env));
  }

  private fail(error: unknown): void {
    this.cancelIdle();
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
      case 'saving':
      case 'stopping':
        await state.pending;
        return this.getRuntime();
      case 'idle': {
        const started = Date.now();
        const pending = Promise.resolve().then(() => this.loadRuntime()).then(runtime => {
          this.startupMilliseconds = Date.now() - started;
          this.starts++;
          this.state = { phase: 'running', runtime };
          return runtime;
        }).catch(error => { this.fail(error); throw error; });
        this.state = { phase: 'starting', pending };
        return pending;
      }
    }
  }

  private cancelIdle(): void {
    clearTimeout(this.idleTimer);
    this.idleTimer = undefined;
    this.idleDeadline = undefined;
  }

  async connect(socket: Socket): Promise<void> {
    void socket.closed.catch(() => undefined);
    this.cancelIdle();
    this.connections++;
    try {
      await (await this.getRuntime()).connect(socket);
    } finally {
      this.connections--;
      await socket.close().catch(() => undefined);
      if (this.connections === 0) await this.checkpoint(false);
    }
  }

  private async checkpoint(stop: boolean): Promise<void> {
    const state = this.state;
    if (state.phase === 'saving' || state.phase === 'stopping') return state.pending;
    if (state.phase !== 'running') return;
    const idleSeconds = readSettings(this.env).idleTimeoutSeconds;
    stop ||= idleSeconds === 0;
    const pending = Promise.resolve().then(async () => {
      if (stop) await state.runtime.stop();
      else await state.runtime.save();
      await this.ctx.storage.sync();
      if (this.state.phase === 'failed') throw new Error(this.state.error);
      this.checkpointedAt = new Date().toISOString();
      this.state = stop ? { phase: 'idle' } : { phase: 'running', runtime: state.runtime };
      if (!stop && this.connections === 0) {
        this.idleDeadline = Date.now() + idleSeconds * 1000;
        this.idleTimer = setTimeout(() => {
          this.cancelIdle();
          if (this.connections === 0) {
            this.ctx.waitUntil(this.checkpoint(true));
          }
        }, idleSeconds * 1000);
      }
    }).catch(error => { this.fail(error); throw error; });
    this.state = stop ? { phase: 'stopping', pending } : { phase: 'saving', runtime: state.runtime, pending };
    return pending;
  }

  /** Server-list traffic reads metadata without instantiating Wasm. */
  serverList(protocol: number) {
    const settings = readSettings(this.env);
    const state = this.status();
    return {
      version: { name: VERSION_NAME, protocol: protocol >= MIN_PROTOCOL && protocol <= MAX_PROTOCOL ? protocol : MIN_PROTOCOL },
      players: { max: settings.maxPlayers, online: state.server?.players ?? 0 },
      description: { text: state.failure ? 'Server unavailable' : settings.motd },
      enforcesSecureChat: false,
    };
  }

  status() {
    let server: ReturnType<Runtime['status']> | null = null;
    if (this.state.phase === 'running' || this.state.phase === 'saving') {
      try { server = this.state.runtime.status(); }
      catch (error) { this.fail(error); }
    }
    let settings: ServerSettings | null = null;
    try { settings = readSettings(this.env); }
    catch (error) { this.fail(error); }
    return {
      phase: this.state.phase,
      connections: this.connections,
      failure: this.state.phase === 'failed' ? this.state.error : null,
      startup_ms: this.startupMilliseconds ?? null,
      runtime_starts: this.starts,
      checkpointed_at: this.checkpointedAt ?? null,
      idle_deadline: this.idleDeadline ?? null,
      settings,
      server,
    };
  }
}

declare module '*.wasm' {
  const module: WebAssembly.Module;
  export default module;
}

declare module '*pumpkin-do.js' {
  interface Options {
    cwd: string;
    nodeFs: typeof import('worker-fs-mount/fs-sync');
    instantiateWasm(imports: WebAssembly.Imports, receive: (instance: WebAssembly.Instance) => void): WebAssembly.Exports;
    printErr(message: string): void;
  }
  interface Pumpkin {
    pumpkin_run(ready: () => void): Promise<void>;
    pumpkin_stop(): void;
    pumpkin_status(): { players: number; ticks: number; wasm_memory_bytes: number };
  }
  export default function create(options: Options): Promise<Pumpkin>;
}

// Routes an inbound socket to the net.Server listening on its local address's
// port within the current Durable Object's port table; resolves when the
// connection closes.
declare module 'cloudflare:node' {
  export function handleAsNodeConnection(socket: Socket): Promise<void>;
}

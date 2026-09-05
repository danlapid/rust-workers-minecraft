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
    pumpkin_start(): Promise<void>;
    pumpkin_connect(socket: Socket): Promise<void>;
    pumpkin_status(): { players: number; ticks: number; wasm_memory_bytes: number };
    pumpkin_shutdown(): Promise<void>;
  }
  export default function create(options: Options): Promise<Pumpkin>;
}

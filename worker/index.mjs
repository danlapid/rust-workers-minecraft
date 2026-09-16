import { DurableObject } from 'cloudflare:workers';
import { connect, fetch, MinecraftWorld as World } from '../target/workers/wasm32-unknown-emscripten/release/pumpkin-do.js';

export default { connect, fetch };

// The runtime enables RPC on a Durable Object class only when it derives from
// DurableObject; the wasm-bindgen class is plain, so wrap it.
export class MinecraftWorld extends DurableObject {
  #world;
  constructor(state, env) {
    super(state, env);
    this.#world = new World(state, env);
  }
  connect(socket) { return this.#world.connect(socket); }
  status() { return this.#world.status(); }
}

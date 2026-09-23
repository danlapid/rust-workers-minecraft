export interface ServerSettings {
  viewDistance: number;
  simulationDistance: number;
  maxPlayers: number;
  compressionThreshold: number;
  compressionLevel: number;
  idleTimeoutSeconds: number;
  motd: string;
  seed?: string;
}

/** Operator settings are read per world, before starting its Wasm instance. */
export function readSettings(env: Partial<Env>): ServerSettings {
  function integer(name: keyof Env, fallback: number, min: number, max: number): number {
    const raw = env[name];
    if (raw === undefined) return fallback;
    if (typeof raw !== 'string' || !/^-?\d+$/.test(raw)) {
      throw new Error(`${String(name)} must be an integer between ${min} and ${max}`);
    }
    const value = Number(raw);
    if (!Number.isSafeInteger(value) || value < min || value > max) {
      throw new Error(`${String(name)} must be an integer between ${min} and ${max}`);
    }
    return value;
  }
  const viewDistance = integer('VIEW_DISTANCE', 4, 2, 32);
  const simulationDistance = integer('SIMULATION_DISTANCE', 3, 2, 32);
  if (simulationDistance > viewDistance) {
    throw new Error('SIMULATION_DISTANCE must not exceed VIEW_DISTANCE');
  }
  const motd = env.MOTD ?? 'Minecraft on Cloudflare Workers';
  if (typeof motd !== 'string' || !motd.trim() || motd.length > 512) {
    throw new Error('MOTD must contain between 1 and 512 characters');
  }
  const seed = 'WORLD_SEED' in env ? env.WORLD_SEED : undefined;
  if (seed !== undefined && (typeof seed !== 'string' || !seed.trim() || seed.length > 256)) {
    throw new Error('WORLD_SEED must be a nonempty string of at most 256 characters');
  }
  return {
    viewDistance,
    simulationDistance,
    maxPlayers: integer('MAX_PLAYERS', 20, 1, 1000),
    compressionThreshold: integer('COMPRESSION_THRESHOLD', 512, -1, 2 * 1024 * 1024),
    compressionLevel: integer('COMPRESSION_LEVEL', 1, 0, 9),
    idleTimeoutSeconds: integer('IDLE_TIMEOUT_SECONDS', 10, 0, 30),
    motd,
    ...(typeof seed === 'string' ? { seed } : {}),
  };
}

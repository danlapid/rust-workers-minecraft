import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readSettings } from '../worker/config.ts';

test('operator settings are parsed without coercing malformed values', () => {
  const settings = readSettings({ VIEW_DISTANCE: '4', SIMULATION_DISTANCE: '2', MAX_PLAYERS: '2', COMPRESSION_THRESHOLD: '-1', COMPRESSION_LEVEL: '1', IDLE_TIMEOUT_SECONDS: '0', MOTD: 'A small world', WORLD_SEED: '42' });
  assert.equal(settings.viewDistance, 4);
  assert.equal(settings.simulationDistance, 2);
  assert.equal(settings.compressionThreshold, -1);
  assert.equal(settings.seed, '42');
  for (const value of ['', '3.5', '3x', 'NaN', 'Infinity', ' 3', '1', '33']) {
    assert.throws(() => readSettings({ VIEW_DISTANCE: value }), /VIEW_DISTANCE/);
  }
  assert.throws(() => readSettings({ VIEW_DISTANCE: '3', SIMULATION_DISTANCE: '4' }), /must not exceed/);
  assert.throws(() => readSettings({ MAX_PLAYERS: '0' }), /MAX_PLAYERS/);
  assert.throws(() => readSettings({ COMPRESSION_LEVEL: '10' }), /COMPRESSION_LEVEL/);
  assert.throws(() => readSettings({ IDLE_TIMEOUT_SECONDS: '31' }), /IDLE_TIMEOUT/);
  assert.throws(() => readSettings({ MOTD: ' ' }), /MOTD/);
  assert.throws(() => readSettings({ WORLD_SEED: 42 }), /WORLD_SEED/);
  assert.throws(() => readSettings({ WORLD_SEED: '' }), /WORLD_SEED/);
});

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Exercise the installed launcher, including Chronicle's --ignore-user-config.
// Isolated home, inert prompt, and loopback endpoint: never sends paid inference.
const executable = process.argv[2];
assert.ok(executable, 'Usage: node scripts/check-automatic-openai-policy.mjs /path/to/codex-external');
const codexHome = mkdtempSync(join(tmpdir(), 'codex-background-policy-'));
try {
  const result = spawnSync(executable, [
    'exec', '--ignore-user-config', '--skip-git-repo-check', '--ephemeral',
    '--model', 'history-policy-test',
    '-c', 'model_provider="openai-memgen"',
    '-c', 'model_providers.openai-memgen.name="History policy test"',
    '-c', 'model_providers.openai-memgen.base_url="http://127.0.0.1:1/v1"',
    '-c', 'model_providers.openai-memgen.wire_api="responses"',
    '-c', 'features.memories=false',
    '-c', 'features.plugins=false',
    '-c', 'features.apps=false',
    '-c', 'mcp_servers={}',
    '-c', 'analytics.enabled=false',
    'This request must be rejected before inference.',
  ], {
    encoding: 'utf8', timeout: 30000,
    env: { ...process.env, CODEX_HOME: codexHome, CODEX_APP_SERVER_TEST_USER_CONFIG_FILE: '' },
  });
  assert.ok(!result.error, `${result.error?.message}\n${result.stderr.slice(-3000)}`);
  assert.notEqual(result.status, 0, 'Background inference should be rejected');
  assert.match(result.stderr + result.stdout, /automatic OpenAI inference is disabled/);
  console.log('PASS: external launcher blocks history inference even with --ignore-user-config');
} finally {
  rmSync(codexHome, { recursive: true, force: true });
}

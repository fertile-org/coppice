#!/usr/bin/env node
/**
 * Render the merged screenshot Compose config and check the effective result.
 * This does not start containers. Exit 2 when the Docker Compose CLI is absent.
 */
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const DEMO_KEY = 'coppice-screenshot-demo-only';
const CANARY = 'HOST-SECRETS-MASTER-KEY-CANARY';
const HOME_CANARY = '/home/host-should-not-appear';

function fail(message) {
  console.error(`screenshot-check: ${message}`);
  process.exit(1);
}

function renderYaml(files, env) {
  const composeArgs = [...files.flatMap((file) => ['-f', file]), 'config'];
  const attempts = [
    ['docker', ['compose', ...composeArgs]],
    ['docker-compose', composeArgs],
  ];
  let rendered = null;
  for (const [command, args] of attempts) {
    const result = spawnSync(command, args, { cwd: root, env, encoding: 'utf8' });
    if (result.error && result.error.code === 'ENOENT') continue;
    rendered = result;
    break;
  }
  if (!rendered) {
    console.error('screenshot-check: docker compose config was not run (docker compose is not installed)');
    process.exit(2);
  }
  if (rendered.status !== 0) {
    fail(rendered.stderr || rendered.stdout || 'docker compose config failed');
  }
  return rendered.stdout;
}

function assertDemoIsolation(yaml, label) {
  if (yaml.includes(CANARY)) fail(`${label} contains the host SECRETS_MASTER_KEY`);
  if (yaml.includes(HOME_CANARY)) fail(`${label} contains the host home path`);
  if (yaml.includes('0.0.0.0')) fail(`${label} publishes on 0.0.0.0`);
  if (!yaml.includes(`SECRETS_MASTER_KEY: ${DEMO_KEY}`) && !yaml.includes(`SECRETS_MASTER_KEY: "${DEMO_KEY}"`)) {
    fail(`${label} does not set SECRETS_MASTER_KEY to ${DEMO_KEY}`);
  }
  if (yaml.includes('deploy/config/config.toml')) fail(`${label} still mounts deploy/config/config.toml`);
}

function assertLoopback(yaml) {
  const services = new Map();
  let service = null;
  let inPorts = false;
  let port = null;
  const flushPort = () => {
    if (!service || !port) return;
    services.get(service).push(port);
    port = null;
  };
  for (const line of yaml.split('\n')) {
    const serviceMatch = line.match(/^  ([a-z0-9-]+):\s*$/);
    if (serviceMatch && !line.startsWith('    ')) {
      flushPort();
      service = serviceMatch[1];
      services.set(service, []);
      inPorts = false;
      continue;
    }
    if (!service) continue;
    if (line === '    ports:') {
      flushPort();
      inPorts = true;
      continue;
    }
    if (inPorts && line === '    networks:' || (inPorts && /^    [a-z_]+:/.test(line) && line !== '    ports:')) {
      flushPort();
      inPorts = false;
    }
    if (!inPorts) continue;
    if (line.startsWith('      - ')) {
      flushPort();
      port = { raw: line.trim() };
      continue;
    }
    if (!port) continue;
    const field = line.match(/^        ([a-z_]+):\s*(.*)$/);
    if (field) port[field[1]] = field[2].replace(/^["']|["']$/g, '');
  }
  flushPort();

  for (const name of ['postgres', 'server', 'web']) {
    const ports = services.get(name) ?? [];
    if (ports.length !== 1) fail(`${name} has ${ports.length} published ports, expected 1 loopback mapping`);
    const published = ports[0];
    if (published.host_ip !== '127.0.0.1') {
      fail(`${name} host_ip is ${published.host_ip ?? '(unset)'} ${published.raw ?? ''}`);
    }
  }
}

const dir = mkdtempSync(path.join(tmpdir(), 'coppice-screenshot.'));
try {
  const prepared = spawnSync('sh', [path.join(root, 'e2e/screenshot/prepare-capture.sh'), dir, path.join(root, 'deploy/config/config.example.toml')], {
    encoding: 'utf8',
  });
  if (prepared.status !== 0) fail(prepared.stderr || 'prepare-capture failed');
  const [repos, config] = prepared.stdout.trim().split('\n');

  const env = { ...process.env, SECRETS_MASTER_KEY: CANARY, HOME: HOME_CANARY, COPPICE_REPOS_HOST: repos, COPPICE_SCREENSHOT_CONFIG: config };
  delete env.COPPICE_PG_PORT;
  delete env.COPPICE_SERVER_PORT;
  delete env.COPPICE_WEB_PORT;
  const bindFiles = ['deploy/docker-compose.yml', 'deploy/docker-compose.screenshot.yml'];
  const yaml = renderYaml(bindFiles, env);
  assertDemoIsolation(yaml, 'bind mode');
  assertLoopback(yaml);

  const reposMount = yaml.includes(repos);
  const configMount = yaml.includes(config);
  if (!reposMount) fail('rendered config does not mount the private repos directory');
  if (!configMount) fail('rendered config does not mount the private config file');
  console.log('screenshot-check: merged config publishes postgres, API, and web on 127.0.0.1 only');
  console.log('screenshot-check: SECRETS_MASTER_KEY is the demo value; host key and home path are absent');

  // The bind file still interpolates its required paths. Copy mode overrides
  // those mounts, so the rendered service has no host file.
  const copyYaml = renderYaml(
    [...bindFiles, 'deploy/docker-compose.screenshot-copy.yml'],
    env,
  );
  assertDemoIsolation(copyYaml, 'copy mode');
  assertLoopback(copyYaml);
  if (copyYaml.includes(repos) || copyYaml.includes(config)) {
    fail('copy mode still mounts the private directory');
  }
  if (copyYaml.includes('type: bind')) fail('copy mode publishes a host bind');
  if (!copyYaml.includes('screenshot_repos:')) fail('copy mode does not mount the project repos volume');
  console.log('screenshot-check: copy mode publishes no host file');
} finally {
  rmSync(dir, { recursive: true, force: true });
}

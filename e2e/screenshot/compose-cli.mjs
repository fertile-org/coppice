/** Compose command for the screenshot scripts.
 * DOCKER_COMPOSE is `docker compose` or `docker-compose`, the same value the Makefile uses.
 * Copy transfer adds the overlay that drops host binds.
 */

export const SCREENSHOT_COMPOSE_FILES = [
  'deploy/docker-compose.yml',
  'deploy/docker-compose.screenshot.yml',
];

export function screenshotComposeFiles() {
  const files = [...SCREENSHOT_COMPOSE_FILES];
  if (process.env.COPPICE_SCREENSHOT_TRANSFER === 'copy') {
    files.push('deploy/docker-compose.screenshot-copy.yml');
  }
  return files;
}

export function composeInvocation(
  extra,
  {
    project = process.env.COPPICE_COMPOSE_PROJECT ?? 'coppice-screenshot',
    files = screenshotComposeFiles(),
  } = {},
) {
  const spec = (process.env.DOCKER_COMPOSE || 'docker compose').trim();
  const parts = spec.split(/\s+/).filter(Boolean);
  if (parts.length === 0) {
    throw new Error('DOCKER_COMPOSE is empty');
  }
  return {
    bin: parts[0],
    args: [...parts.slice(1), '-p', project, ...files.flatMap((file) => ['-f', file]), ...extra],
  };
}

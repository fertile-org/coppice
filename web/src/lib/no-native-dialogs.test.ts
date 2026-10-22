import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { ESLint } from 'eslint';
import { describe, expect, it } from 'vitest';

const webRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const natives = ['confirm', 'alert', 'prompt'] as const;

async function lintSnippet(source: string) {
  const eslint = new ESLint({ cwd: webRoot });
  const [result] = await eslint.lintText(source, {
    filePath: path.join(webRoot, 'src/lint-native-dialogs.tsx'),
  });
  return result?.messages ?? [];
}

describe('native dialog lint rule', () => {
  it('fails on window-qualified native dialogs', async () => {
    const messages = await lintSnippet(
      `export function bad() {\n${natives.map((name) => `  window.${name}("x");`).join('\n')}\n}\n`,
    );
    const restricted = messages.filter(
      (message) => message.ruleId === 'no-restricted-properties',
    );
    expect(restricted).toHaveLength(3);
  });

  it('fails on bare native dialogs', async () => {
    const messages = await lintSnippet(
      `export function bad() {\n${natives.map((name) => `  ${name}("x");`).join('\n')}\n}\n`,
    );
    const restricted = messages.filter(
      (message) => message.ruleId === 'no-restricted-globals',
    );
    expect(restricted).toHaveLength(3);
  });
});

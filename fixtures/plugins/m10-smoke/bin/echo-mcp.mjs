#!/usr/bin/env node
// Minimal MCP server over stdio (newline-delimited JSON-RPC) for `make e2e-smoke-m10`.
// One read-only tool, `echo`, which answers `${GREETING}: ${text}`.

import { createInterface } from 'node:readline';

const ECHO_TOOL = {
  name: 'echo',
  description: 'Echo args.text, prefixed with the GREETING setting',
  inputSchema: {
    type: 'object',
    properties: { text: { type: 'string' } },
  },
  annotations: { readOnlyHint: true },
};

function send(message) {
  process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', ...message })}\n`);
}

function handle(method, params) {
  switch (method) {
    case 'initialize':
      return {
        protocolVersion: params?.protocolVersion ?? '2025-06-18',
        capabilities: { tools: {} },
        serverInfo: { name: 'm10-smoke-echo', version: '0.1.0' },
      };
    case 'ping':
      return {};
    case 'tools/list':
      return { tools: [ECHO_TOOL] };
    case 'tools/call': {
      if (params?.name !== 'echo') {
        return {
          content: [{ type: 'text', text: `unknown tool ${params?.name}` }],
          isError: true,
        };
      }
      const text = params.arguments?.text ?? '';
      return {
        content: [{ type: 'text', text: `${process.env.GREETING}: ${text}` }],
        isError: false,
      };
    }
    default:
      return undefined;
  }
}

const lines = createInterface({ input: process.stdin });
lines.on('line', (line) => {
  if (!line.trim()) {
    return;
  }
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    return;
  }
  if (message.id === undefined || message.id === null) {
    return;
  }
  const result = handle(message.method, message.params);
  if (result === undefined) {
    send({
      id: message.id,
      error: { code: -32601, message: `method not found: ${message.method}` },
    });
    return;
  }
  send({ id: message.id, result });
});

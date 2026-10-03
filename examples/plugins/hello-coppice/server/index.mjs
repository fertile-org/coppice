#!/usr/bin/env node
// Minimal MCP server over stdio (newline-delimited JSON-RPC 2.0), no dependencies.
// Logs go to stderr; stdout carries protocol messages only.
import { createInterface } from 'node:readline';

const greeting = process.env.HELLO_GREETING || 'Hello';

const tools = [
  {
    name: 'greet',
    description: 'Returns a greeting for the given name.',
    inputSchema: {
      type: 'object',
      properties: { name: { type: 'string', description: 'Who to greet' } },
      required: ['name'],
    },
    annotations: { readOnlyHint: true },
  },
];

function send(message) {
  process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', ...message })}\n`);
}

function handle(method, params) {
  switch (method) {
    case 'initialize':
      return {
        protocolVersion: params?.protocolVersion ?? '2025-06-18',
        capabilities: { tools: {} },
        serverInfo: { name: 'hello-coppice', version: '0.1.0' },
      };
    case 'ping':
      return {};
    case 'tools/list':
      return { tools };
    case 'tools/call': {
      if (params?.name !== 'greet') {
        return {
          content: [{ type: 'text', text: `unknown tool ${params?.name}` }],
          isError: true,
        };
      }
      const name = String(params.arguments?.name ?? '').trim() || 'there';
      return { content: [{ type: 'text', text: `${greeting}, ${name}!` }] };
    }
    default:
      return undefined;
  }
}

createInterface({ input: process.stdin }).on('line', (line) => {
  if (!line.trim()) return;
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    console.error('hello-coppice: ignoring invalid JSON');
    return;
  }
  if (message.id === undefined) return;
  const result = handle(message.method, message.params);
  if (result === undefined) {
    send({ id: message.id, error: { code: -32601, message: `method not found: ${message.method}` } });
  } else {
    send({ id: message.id, result });
  }
});

console.error('hello-coppice: ready');

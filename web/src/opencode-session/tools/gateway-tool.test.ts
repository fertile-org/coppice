import { describe, expect, it } from 'vitest';
import { gatewayToolTitle, parseGatewayTool } from './gateway-tool';

describe('parseGatewayTool', () => {
  it('parses names and builds titles', () => {
    expect(parseGatewayTool('coppice_ticket_get')).toEqual({ plugin: null, tool: 'ticket_get' });
    expect(parseGatewayTool('coppice_github__create_issue')).toEqual({
      plugin: 'github',
      tool: 'create_issue',
    });
    expect(parseGatewayTool('bash')).toBeNull();
    expect(gatewayToolTitle({ plugin: null, tool: 'ticket_get' })).toContain('ticket_get');
    expect(gatewayToolTitle({ plugin: 'github', tool: 'create_issue' })).toContain('github');
  });
});

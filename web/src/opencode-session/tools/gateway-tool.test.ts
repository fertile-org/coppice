import { describe, expect, it } from 'vitest';
import { gatewayToolTitle, parseGatewayTool } from './gateway-tool';

describe('parseGatewayTool', () => {
  it('parses core gateway tools', () => {
    expect(parseGatewayTool('coppice_ticket_get')).toEqual({ plugin: null, tool: 'ticket_get' });
  });

  it('parses plugin tools on the first double underscore', () => {
    expect(parseGatewayTool('coppice_github__create_issue')).toEqual({
      plugin: 'github',
      tool: 'create_issue',
    });
  });

  it('ignores non-gateway tools', () => {
    expect(parseGatewayTool('bash')).toBeNull();
    expect(parseGatewayTool('coppice_')).toBeNull();
  });

  it('titles core and plugin tools', () => {
    expect(gatewayToolTitle({ plugin: null, tool: 'ticket_get' })).toBe('coppice · ticket_get');
    expect(gatewayToolTitle({ plugin: 'github', tool: 'create_issue' })).toBe(
      'github · create_issue',
    );
  });
});

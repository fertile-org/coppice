import type { ComponentProps } from 'react';
import type { ConnectorConsole } from '../../lib/schemas/connector';
import { ClaudeLiveConsole } from './ClaudeLiveConsole';
import { LiveConsole } from './LiveConsole';
import { LiveSession } from './LiveSession';

type LiveRunViewProps = ComponentProps<typeof LiveConsole> & {
  console: ConnectorConsole | undefined;
};

export function LiveRunView({ console, ...props }: LiveRunViewProps) {
  switch (console) {
    case 'openCodeSession':
      return <LiveSession {...props} />;
    case 'structured':
      return <ClaudeLiveConsole {...props} />;
    default:
      return <LiveConsole {...props} />;
  }
}

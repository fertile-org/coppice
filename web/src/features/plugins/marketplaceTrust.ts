import type { MarketplaceSource } from '../../lib/schemas/marketplace';
import { PLUGIN_COPY } from './copy';

export function isCoppiceSource(source: MarketplaceSource | undefined): boolean {
  return source?.kind === 'builtin';
}

export function trustLabel(source: MarketplaceSource | undefined): string {
  return isCoppiceSource(source) ? PLUGIN_COPY.trustCoppice : PLUGIN_COPY.trustCommunity;
}

import { z } from 'zod';

export const pluginDirSchema = z.object({
  id: z.string().uuid(),
  path: z.string(),
  position: z.number().int(),
  isDefault: z.boolean(),
});

export type PluginDir = z.infer<typeof pluginDirSchema>;

export const pluginStatusSchema = z.enum([
  'ok',
  'invalid',
  'missing',
  'shadowed',
  'external',
]);

export type PluginStatus = z.infer<typeof pluginStatusSchema>;

export const pluginSkillSchema = z.object({
  name: z.string(),
  description: z.string(),
  relPath: z.string(),
  error: z.string().nullable(),
  enabled: z.boolean().default(true),
});

export type PluginSkill = z.infer<typeof pluginSkillSchema>;

export const pluginMcpServerKindSchema = z
  .enum(['stdio', 'http', 'sse', 'unknown'])
  .catch('unknown');

export const pluginMcpServerHealthSchema = z
  .enum(['stopped', 'starting', 'ready', 'backoff', 'unhealthy'])
  .catch('stopped');

export type PluginMcpServerHealth = z.infer<typeof pluginMcpServerHealthSchema>;

export const pluginMcpServerSchema = z.object({
  name: z.string(),
  kind: pluginMcpServerKindSchema,
  health: pluginMcpServerHealthSchema,
});

export type PluginMcpServer = z.infer<typeof pluginMcpServerSchema>;

export const pluginSettingSourceSchema = z
  .enum(['setting', 'env', 'default', 'missing'])
  .catch('missing');

export type PluginSettingSource = z.infer<typeof pluginSettingSourceSchema>;

export const pluginSettingSchema = z.object({
  key: z.string(),
  configured: z.boolean(),
  source: pluginSettingSourceSchema,
});

export type PluginSetting = z.infer<typeof pluginSettingSchema>;

export const pluginSchema = z.object({
  id: z.string().uuid(),
  pluginDirId: z.string().uuid(),
  relPath: z.string(),
  name: z.string(),
  version: z.string(),
  description: z.string(),
  source: z.enum(['local', 'git']),
  gitUrl: z.string().nullable(),
  gitRef: z.string().nullable(),
  gitCommit: z.string().nullable(),
  status: pluginStatusSchema,
  error: z.string().nullable(),
  enabled: z.boolean(),
  skills: z.array(pluginSkillSchema),
  mcpServers: z.array(pluginMcpServerSchema),
  settings: z.array(pluginSettingSchema).default([]),
  unsupported: z.array(z.string()),
  marketplace: z.object({ name: z.string() }).nullable().default(null),
  external: z
    .object({ kind: z.string(), url: z.string().nullable() })
    .nullable()
    .default(null),
  gitRoot: z.string().nullable().default(null),
  agentAccess: z.enum(['all', 'explicit']).default('explicit'),
  agentIds: z.array(z.string().uuid()).default([]),
});

export type Plugin = z.infer<typeof pluginSchema>;

export const pluginTestToolSchema = z.object({
  name: z.string(),
  exposedName: z.string(),
  description: z.string(),
  readOnly: z.boolean(),
});

export const pluginTestServerSchema = z.object({
  name: z.string(),
  kind: pluginMcpServerKindSchema,
  status: z.enum(['ok', 'error', 'unsupported']),
  error: z.string().nullish(),
  tools: z.array(pluginTestToolSchema).default([]),
});

export type PluginTestServer = z.infer<typeof pluginTestServerSchema>;

export const pluginTestResultSchema = z.object({
  servers: z.array(pluginTestServerSchema),
});

export type PluginTestResult = z.infer<typeof pluginTestResultSchema>;

export const pluginInstallStatusSchema = z.enum(['running', 'succeeded', 'failed']);

export type PluginInstallStatus = z.infer<typeof pluginInstallStatusSchema>;

export const pluginInstallSchema = z.object({
  id: z.string().uuid(),
  pluginDirId: z.string().uuid(),
  kind: z.string(),
  gitUrl: z.string(),
  gitRef: z.string().nullable(),
  pluginId: z.string().uuid().nullable(),
  pluginIds: z.array(z.string().uuid()).default([]),
  status: pluginInstallStatusSchema,
  error: z.string().nullable(),
});

export type PluginInstall = z.infer<typeof pluginInstallSchema>;

export type InstallPluginInput = {
  gitUrl: string;
  ref?: string;
  pluginDirId: string;
};

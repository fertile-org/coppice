import { z } from 'zod';

export const pluginDirSchema = z.object({
  id: z.string().uuid(),
  path: z.string(),
  position: z.number().int(),
  isDefault: z.boolean(),
});

export type PluginDir = z.infer<typeof pluginDirSchema>;

export const pluginStatusSchema = z.enum(['ok', 'invalid', 'missing', 'shadowed']);

export type PluginStatus = z.infer<typeof pluginStatusSchema>;

export const pluginSkillSchema = z.object({
  name: z.string(),
  description: z.string(),
  relPath: z.string(),
  error: z.string().nullable(),
});

export type PluginSkill = z.infer<typeof pluginSkillSchema>;

export const pluginMcpServerSchema = z.object({
  name: z.string(),
  kind: z.enum(['stdio', 'http', 'sse', 'unknown']),
});

export type PluginMcpServer = z.infer<typeof pluginMcpServerSchema>;

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
  unsupported: z.array(z.string()),
});

export type Plugin = z.infer<typeof pluginSchema>;

export const pluginInstallStatusSchema = z.enum(['running', 'succeeded', 'failed']);

export type PluginInstallStatus = z.infer<typeof pluginInstallStatusSchema>;

export const pluginInstallSchema = z.object({
  id: z.string().uuid(),
  pluginDirId: z.string().uuid(),
  kind: z.string(),
  gitUrl: z.string(),
  gitRef: z.string().nullable(),
  pluginId: z.string().uuid().nullable(),
  status: pluginInstallStatusSchema,
  error: z.string().nullable(),
});

export type PluginInstall = z.infer<typeof pluginInstallSchema>;

export type InstallPluginInput = {
  gitUrl: string;
  ref?: string;
  pluginDirId: string;
};

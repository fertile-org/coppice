import { z } from 'zod';

export const SUBSTATUSES = [
  'waiting_for_agent',
  'waiting_for_human',
  'waiting_for_owner',
  'waiting_for_ci',
  'blocked_by_missing_capability',
  'blocked_by_missing_secret',
  'blocked_by_permission',
  'blocked_by_error',
] as const;

export type Substatus = (typeof SUBSTATUSES)[number];

export const substatusSchema = z.enum(SUBSTATUSES);

export const SUBSTATUS_LABELS: Record<Substatus, string> = {
  waiting_for_agent: 'Waiting for agent',
  waiting_for_human: 'Waiting for you',
  waiting_for_owner: 'Waiting for owner',
  waiting_for_ci: 'Waiting for CI',
  blocked_by_missing_capability: 'Blocked — capability',
  blocked_by_missing_secret: 'Blocked — secret',
  blocked_by_permission: 'Blocked — permission',
  blocked_by_error: 'Blocked — error',
};

const uuidSchema = z.string().uuid();

export const substatusMetadataSchema = z
  .object({
    substatus: substatusSchema,
    metadata: z.record(z.unknown()),
  })
  .superRefine((data, ctx) => {
    switch (data.substatus) {
      case 'waiting_for_agent': {
        const agentId = data.metadata.agentId;
        if (typeof agentId !== 'string' || !uuidSchema.safeParse(agentId).success) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            message: 'agentId required',
            path: ['metadata', 'agentId'],
          });
        }
        break;
      }
      case 'blocked_by_missing_capability': {
        const capability = data.metadata.capability;
        if (typeof capability !== 'string' || capability.length === 0) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            message: 'capability required',
            path: ['metadata', 'capability'],
          });
        }
        break;
      }
      case 'blocked_by_missing_secret': {
        const secretKey = data.metadata.secretKey;
        if (typeof secretKey !== 'string' || secretKey.length === 0) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            message: 'secretKey required',
            path: ['metadata', 'secretKey'],
          });
        }
        break;
      }
      default:
        break;
    }
  });

export function substatusRequiresMetadata(substatus: Substatus): boolean {
  return (
    substatus === 'waiting_for_agent' ||
    substatus === 'blocked_by_missing_capability' ||
    substatus === 'blocked_by_missing_secret'
  );
}

export function substatusOptionalReason(substatus: Substatus): boolean {
  return substatus === 'waiting_for_human' || substatus === 'waiting_for_owner';
}

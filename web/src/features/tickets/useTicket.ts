import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import type { CreateCommentInput, UpdateStatusInput, UpdateTicketInput } from '../../lib/schemas/ticket';
import { ticketsQueryKey, type Ticket } from '../board/useTickets';
import type { TicketStatus } from '../board/columns';
import { agentRunsQueryKey, upsertAgentRunInCache } from './useAgentRuns';
import type { AgentRun } from '../../lib/schemas/agentRun';

export { useAgents, type AgentSummary } from '../agents/useAgents';

export interface CommentAttachment {
  id: string;
  filename: string;
  contentType: string;
  sizeBytes: number;
}

export interface Comment {
  id: string;
  ticketId: string;
  authorType: 'human' | 'agent' | 'system';
  authorId?: string;
  body: string;
  intent: string;
  mentions: unknown;
  attachmentIds: string[];
  attachments: CommentAttachment[];
  createdAt: string;
}

export interface AttachmentUpload {
  id: string;
  filename: string;
  contentType: string;
  sizeBytes: number;
}

export interface StartedRunSummary {
  runId: string;
  agentId: string;
  agentKey: string;
  jobType: string;
}

export interface CreateCommentResponse extends Comment {
  startedRuns?: StartedRunSummary[];
}

export function ticketQueryKey(ticketId: string) {
  return ['ticket', ticketId] as const;
}

export function commentsQueryKey(ticketId: string) {
  return ['comments', ticketId] as const;
}

export function gitInfoQueryKey(ticketId: string) {
  return ['ticket-git-info', ticketId] as const;
}

export function childrenQueryKey(ticketId: string) {
  return ['ticket-children', ticketId] as const;
}

export interface TicketGitInfo {
  ticketBranch: string;
  worktreePath: string;
  worktreeExists: boolean;
  defaultBranch: string;
  branches: string[];
  remoteUrl?: string | null;
  prCreateUrl?: string | null;
}

export interface MergeBranchResponse {
  merge: {
    baseBranch: string;
    ticketBranch: string;
    headSha: string;
    message: string;
  };
}

async function fetchTicket(ticketId: string): Promise<Ticket> {
  const res = await apiFetch(`/api/tickets/${ticketId}`);
  return res.json() as Promise<Ticket>;
}

async function patchTicket(
  ticketId: string,
  body: UpdateTicketInput,
): Promise<Ticket> {
  const res = await apiFetch(`/api/tickets/${ticketId}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  return res.json() as Promise<Ticket>;
}

async function patchTicketStatus(
  ticketId: string,
  body: UpdateStatusInput,
): Promise<Ticket> {
  const res = await apiFetch(`/api/tickets/${ticketId}/status`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  return res.json() as Promise<Ticket>;
}

async function fetchComments(ticketId: string): Promise<Comment[]> {
  const res = await apiFetch(`/api/tickets/${ticketId}/comments`);
  return res.json() as Promise<Comment[]>;
}

async function postComment(
  ticketId: string,
  body: CreateCommentInput,
): Promise<CreateCommentResponse> {
  const res = await apiFetch(`/api/tickets/${ticketId}/comments`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  return res.json() as Promise<CreateCommentResponse>;
}

async function uploadAttachment(file: File): Promise<AttachmentUpload> {
  const formData = new FormData();
  formData.append('file', file);
  const res = await apiFetch('/api/attachments', {
    method: 'POST',
    body: formData,
  });
  return res.json() as Promise<AttachmentUpload>;
}

async function assignTicketAgent(
  ticketId: string,
  agentId: string | null,
): Promise<Ticket> {
  const res = await apiFetch(`/api/tickets/${ticketId}/assign`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ agentId }),
  });
  return res.json() as Promise<Ticket>;
}

async function postFinalApprove(ticketId: string): Promise<Ticket> {
  const res = await apiFetch(`/api/tickets/${ticketId}/final-approve`, {
    method: 'POST',
  });
  return res.json() as Promise<Ticket>;
}

async function fetchTicketGitInfo(ticketId: string): Promise<TicketGitInfo> {
  const res = await apiFetch(`/api/tickets/${ticketId}/git-info`);
  return res.json() as Promise<TicketGitInfo>;
}

async function postMergeBranch(
  ticketId: string,
  baseBranch: string,
): Promise<MergeBranchResponse> {
  const res = await apiFetch(`/api/tickets/${ticketId}/merge-branch`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ baseBranch }),
  });
  return res.json() as Promise<MergeBranchResponse>;
}

async function postRemoveWorktree(ticketId: string): Promise<TicketGitInfo> {
  const res = await apiFetch(`/api/tickets/${ticketId}/remove-worktree`, {
    method: 'POST',
  });
  return res.json() as Promise<TicketGitInfo>;
}

async function fetchTicketChildren(ticketId: string): Promise<Ticket[]> {
  const res = await apiFetch(`/api/tickets/${ticketId}/children`);
  return res.json() as Promise<Ticket[]>;
}

async function postApproveSplits(ticketId: string): Promise<Ticket[]> {
  const res = await apiFetch(`/api/tickets/${ticketId}/approve-splits`, {
    method: 'POST',
  });
  return res.json() as Promise<Ticket[]>;
}

async function postDismissSplits(ticketId: string): Promise<Ticket> {
  const res = await apiFetch(`/api/tickets/${ticketId}/dismiss-splits`, {
    method: 'POST',
  });
  return res.json() as Promise<Ticket>;
}

export function useTicket(ticketId: string | undefined) {
  return useQuery({
    queryKey: ticketQueryKey(ticketId ?? ''),
    queryFn: () => fetchTicket(ticketId!),
    enabled: Boolean(ticketId),
  });
}

export function useComments(ticketId: string | undefined) {
  return useQuery({
    queryKey: commentsQueryKey(ticketId ?? ''),
    queryFn: () => fetchComments(ticketId!),
    enabled: Boolean(ticketId),
  });
}

export function useAssignAgent(ticketId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (agentId: string | null) =>
      assignTicketAgent(ticketId, agentId),
    onSuccess: (ticket) => {
      queryClient.setQueryData(ticketQueryKey(ticketId), ticket);
      void queryClient.invalidateQueries({
        queryKey: ['tickets', ticket.projectId],
      });
    },
  });
}

export function useTicketChildren(ticketId: string | undefined) {
  return useQuery({
    queryKey: childrenQueryKey(ticketId ?? ''),
    queryFn: () => fetchTicketChildren(ticketId!),
    enabled: Boolean(ticketId),
  });
}

export function useApproveSplits(ticketId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => postApproveSplits(ticketId),
    onSuccess: (children) => {
      queryClient.setQueryData(childrenQueryKey(ticketId), children);
      void queryClient.invalidateQueries({ queryKey: ticketQueryKey(ticketId) });
      const projectId = children[0]?.projectId;
      if (projectId) {
        void queryClient.invalidateQueries({
          queryKey: ticketsQueryKey(projectId),
        });
      }
    },
  });
}

export function useDismissSplits(ticketId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => postDismissSplits(ticketId),
    onSuccess: (ticket) => {
      queryClient.setQueryData(ticketQueryKey(ticketId), ticket);
    },
  });
}

export function useFinalApprove(ticketId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => postFinalApprove(ticketId),
    onSuccess: (ticket) => {
      queryClient.setQueryData(ticketQueryKey(ticketId), ticket);
      void queryClient.invalidateQueries({
        queryKey: ['tickets', ticket.projectId],
      });
      void queryClient.invalidateQueries({
        queryKey: commentsQueryKey(ticketId),
      });
    },
  });
}

export function useTicketGitInfo(ticketId: string, enabled: boolean) {
  return useQuery({
    queryKey: gitInfoQueryKey(ticketId),
    queryFn: () => fetchTicketGitInfo(ticketId),
    enabled,
  });
}

export function useMergeTicketBranch(ticketId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (baseBranch: string) => postMergeBranch(ticketId, baseBranch),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: gitInfoQueryKey(ticketId) });
      void queryClient.invalidateQueries({ queryKey: commentsQueryKey(ticketId) });
    },
  });
}

export function useRemoveWorktree(ticketId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => postRemoveWorktree(ticketId),
    onSuccess: (info) => {
      queryClient.setQueryData(gitInfoQueryKey(ticketId), info);
      void queryClient.invalidateQueries({ queryKey: commentsQueryKey(ticketId) });
    },
  });
}

export function useUpdateTicket(ticketId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (body: UpdateTicketInput) => patchTicket(ticketId, body),
    onSuccess: (ticket) => {
      queryClient.setQueryData(ticketQueryKey(ticketId), ticket);
    },
  });
}

export function useUpdateTicketStatus(ticketId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (body: UpdateStatusInput) => patchTicketStatus(ticketId, body),
    onSuccess: (ticket) => {
      queryClient.setQueryData(ticketQueryKey(ticketId), ticket);
    },
  });
}

export function useCreateComment(ticketId: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (body: CreateCommentInput) => postComment(ticketId, body),
    onMutate: async () => {
      await queryClient.cancelQueries({ queryKey: commentsQueryKey(ticketId) });
    },
    onSuccess: (data) => {
      const { startedRuns, ...comment } = data;
      queryClient.setQueryData<Comment[]>(commentsQueryKey(ticketId), (old) => {
        const withoutDuplicate = (old ?? []).filter((item) => item.id !== comment.id);
        return [comment, ...withoutDuplicate];
      });
      void queryClient.invalidateQueries({ queryKey: commentsQueryKey(ticketId) });

      if (startedRuns?.length) {
        const now = new Date().toISOString();
        for (const started of startedRuns) {
          const placeholder: AgentRun = {
            id: started.runId,
            ticketId,
            agentId: started.agentId,
            jobType: started.jobType,
            status: 'queued',
            sandboxProfileId: 'permissive-default',
            worktreePath: null,
            branchName: null,
            startedAt: null,
            endedAt: null,
            createdAt: now,
            errorMessage: null,
            sessionId: null,
          };
          upsertAgentRunInCache(queryClient, ticketId, placeholder);
        }
        void queryClient.invalidateQueries({
          queryKey: agentRunsQueryKey(ticketId),
        });
      }
    },
  });
}

export function useUploadAttachment() {
  return useMutation({
    mutationFn: uploadAttachment,
  });
}

export function statusLabel(status: TicketStatus): string {
  const labels: Record<TicketStatus, string> = {
    backlog: 'Backlog',
    ready: 'Ready',
    in_progress: 'In Progress',
    in_review: 'In Review',
    in_qa: 'In QA',
    wait_for_final_review: 'Wait for Final Review',
    done: 'Done',
    blocked: 'Blocked',
  };
  return labels[status];
}

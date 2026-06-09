import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api';
import type { CreateCommentInput, UpdateStatusInput, UpdateTicketInput } from '../../lib/schemas/ticket';
import type { Ticket } from '../board/useTickets';
import type { TicketStatus } from '../board/columns';

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

export function ticketQueryKey(ticketId: string) {
  return ['ticket', ticketId] as const;
}

export function commentsQueryKey(ticketId: string) {
  return ['comments', ticketId] as const;
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
): Promise<Comment> {
  const res = await apiFetch(`/api/tickets/${ticketId}/comments`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  return res.json() as Promise<Comment>;
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
    onSuccess: (comment) => {
      queryClient.setQueryData<Comment[]>(commentsQueryKey(ticketId), (old) =>
        old ? [...old, comment] : [comment],
      );
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

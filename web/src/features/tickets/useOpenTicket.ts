import { useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { apiFetch } from '../../lib/api';
import type { Ticket } from '../board/useTickets';
import { ticketQueryKey } from './useTicket';

export function useOpenTicket() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  return useCallback(
    async (ticketId: string) => {
      let ticket = queryClient.getQueryData<Ticket>(ticketQueryKey(ticketId));
      if (!ticket) {
        const response = await apiFetch(`/api/tickets/${ticketId}`);
        ticket = (await response.json()) as Ticket;
        queryClient.setQueryData(ticketQueryKey(ticketId), ticket);
      }
      navigate(`/projects/${ticket.projectId}/board?ticket=${ticketId}`);
    },
    [navigate, queryClient],
  );
}

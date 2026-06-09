import { useEffect } from 'react';
import { queryClient } from '../../lib/query-client';

export interface AgentRunStartedPayload {
  type: 'agent_run.started';
  run_id: string;
  ticket_id: string;
  agent_id: string;
  status: string;
}

export interface AgentRunFinishedPayload {
  type: 'agent_run.finished';
  run_id: string;
  ticket_id: string;
  agent_id: string;
  status: string;
  error_message?: string | null;
}

interface EventSocketListener {
  onRunStarted?: (payload: AgentRunStartedPayload) => void;
  onRunFinished?: (payload: AgentRunFinishedPayload) => void;
}

const listeners = new Set<EventSocketListener>();
let socket: WebSocket | null = null;
let subscriberCount = 0;

function dispatchMessage(raw: string) {
  const msg = JSON.parse(raw) as { type?: string; ticket_id?: string };

  if (msg.type === 'agent_run.started') {
    const payload = msg as AgentRunStartedPayload;
    for (const listener of listeners) {
      listener.onRunStarted?.(payload);
    }
  }

  if (msg.type === 'agent_run.finished') {
    const payload = msg as AgentRunFinishedPayload;
    for (const listener of listeners) {
      listener.onRunFinished?.(payload);
    }
  }

  if (msg.type === 'ticket.updated') {
    void queryClient.invalidateQueries({ queryKey: ['tickets'] });
    if (msg.ticket_id) {
      void queryClient.invalidateQueries({
        queryKey: ['ticket', msg.ticket_id],
      });
    }
  }

  if (msg.type === 'comment.created' && msg.ticket_id) {
    void queryClient.invalidateQueries({
      queryKey: ['comments', msg.ticket_id],
    });
  }
}

export function dispatchMessageForTest(raw: string) {
  dispatchMessage(raw);
}

function connectSocket() {
  if (socket) return;

  const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
  socket = new WebSocket(`${protocol}://${window.location.host}/ws/events`);

  socket.onmessage = (event) => {
    dispatchMessage(event.data as string);
  };

  socket.onclose = () => {
    socket = null;
    if (subscriberCount > 0) {
      connectSocket();
    }
  };
}

function disconnectSocket() {
  socket?.close();
  socket = null;
}

export function useEventSocket(opts: {
  enabled: boolean;
  onRunStarted?: (payload: AgentRunStartedPayload) => void;
  onRunFinished?: (payload: AgentRunFinishedPayload) => void;
}) {
  const { enabled, onRunStarted, onRunFinished } = opts;

  useEffect(() => {
    if (!enabled) return;

    const listener: EventSocketListener = { onRunStarted, onRunFinished };
    listeners.add(listener);
    subscriberCount += 1;
    connectSocket();

    return () => {
      listeners.delete(listener);
      subscriberCount -= 1;
      if (subscriberCount === 0) {
        disconnectSocket();
      }
    };
  }, [enabled, onRunStarted, onRunFinished]);
}

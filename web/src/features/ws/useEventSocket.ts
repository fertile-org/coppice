import { useEffect } from 'react';
import { queryClient } from '../../lib/query-client';
import {
  patchAgentRunStatusInCache,
} from '../tickets/useAgentRuns';
import type { RunStatus } from '../../lib/schemas/agentRun';

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
let reconnectTimer: number | null = null;
const RECONNECT_DELAY_MS = 1000;

function dispatchMessage(raw: string) {
  const msg = JSON.parse(raw) as { type?: string; ticket_id?: string };

  if (msg.type === 'agent_run.started') {
    const payload = msg as AgentRunStartedPayload;
    patchAgentRunStatusInCache(
      queryClient,
      payload.ticket_id,
      payload.run_id,
      payload.status as RunStatus,
    );
    void queryClient.invalidateQueries({
      queryKey: ['agent-runs', payload.ticket_id],
    });
    void queryClient.invalidateQueries({
      queryKey: ['ticket', payload.ticket_id],
    });
    void queryClient.invalidateQueries({ queryKey: ['tickets'] });
    for (const listener of listeners) {
      listener.onRunStarted?.(payload);
    }
  }

  if (msg.type === 'agent_run.finished') {
    const payload = msg as AgentRunFinishedPayload;
    patchAgentRunStatusInCache(
      queryClient,
      payload.ticket_id,
      payload.run_id,
      payload.status as RunStatus,
    );
    void queryClient.invalidateQueries({
      queryKey: ['agent-runs', payload.ticket_id],
    });
    void queryClient.invalidateQueries({ queryKey: ['tickets'] });
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

  // Server signalled that this socket's view may be stale (e.g. its broadcast
  // receiver lagged). Invalidate everything so the next render refetches truth.
  if (msg.type === 'resync') {
    void queryClient.invalidateQueries();
  }
}

export function dispatchMessageForTest(raw: string) {
  dispatchMessage(raw);
}

function scheduleReconnect() {
  if (reconnectTimer || subscriberCount === 0) return;
  reconnectTimer = window.setTimeout(() => {
    reconnectTimer = null;
    connectSocket();
  }, RECONNECT_DELAY_MS);
}

function connectSocket() {
  if (socket || subscriberCount === 0) return;

  const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
  socket = new WebSocket(`${protocol}://${window.location.host}/ws/events`);

  socket.onmessage = (event) => {
    dispatchMessage(event.data as string);
  };

  socket.onclose = () => {
    socket = null;
    scheduleReconnect();
  };

  socket.onerror = () => {
    socket?.close();
  };
}

function disconnectSocket() {
  if (reconnectTimer) {
    window.clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
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

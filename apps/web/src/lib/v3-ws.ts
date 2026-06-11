import { useEffect, useRef, useCallback, useState } from "react";
import { getAuthState, API_BASE_URL } from "./api";

export interface GroupEvent {
  type: string;
  group_id?: string;
  message_id?: string;
  sender_id?: string;
  content?: string;
  approval_id?: string;
  voter_id?: string;
  decision?: string;
  approved?: boolean;
  task_id?: string;
  old_status?: string;
  new_status?: string;
  member_id?: string;
}

type EventHandler = (event: GroupEvent) => void;
type Transport = "ws" | "sse" | "none";

function dispatchEvent(data: GroupEvent, handlers: {
  onMessage?: EventHandler;
  onApproval?: EventHandler;
  onTask?: EventHandler;
  onMember?: EventHandler;
}) {
  const type = data.type;
  if (type.startsWith("group.message.")) {
    handlers.onMessage?.(data);
  } else if (type.startsWith("approval.")) {
    handlers.onApproval?.(data);
  } else if (type.startsWith("task.")) {
    handlers.onTask?.(data);
  } else if (type.startsWith("group.member.")) {
    handlers.onMember?.(data);
  }
}

/**
 * Hook for real-time v3 group chat events.
 * Tries WebSocket first, falls back to SSE if WS is unavailable.
 */
export function useGroupWebSocket(handlers: {
  onMessage?: EventHandler;
  onApproval?: EventHandler;
  onTask?: EventHandler;
  onMember?: EventHandler;
  onConnect?: () => void;
  onDisconnect?: () => void;
}) {
  const wsRef = useRef<WebSocket | null>(null);
  const esRef = useRef<EventSource | null>(null);
  const subscribedRef = useRef<Set<string>>(new Set());
  const [connected, setConnected] = useState(false);
  const [transport, setTransport] = useState<Transport>("none");
  const handlersRef = useRef(handlers);
  handlersRef.current = handlers;

  useEffect(() => {
    const { token } = getAuthState();
    const authParam = token ? `?token=${token}` : "";

    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const wsUrl = `${protocol}//${window.location.host}/ws/groups${authParam}`;
    let wsFailed = false;
    let cleaned = false;

    function startSSE() {
      if (cleaned) return;
      const groupIds = Array.from(subscribedRef.current);
      const groupParam = groupIds.length > 0 ? `&group_id=${groupIds.join(",")}` : "";
      const sseUrl = `${API_BASE_URL}/api/v3/events${authParam}${authParam ? "&" : "?"}format=text${groupParam}`;
      const es = new EventSource(sseUrl);
      esRef.current = es;

      es.onopen = () => {
        setConnected(true);
        setTransport("sse");
        handlersRef.current.onConnect?.();
      };

      es.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data) as GroupEvent;
          dispatchEvent(data, handlersRef.current);
        } catch { /* ignore non-JSON */ }
      };

      const eventTypes = [
        "group.message.sent", "group.message.edited", "group.message.deleted",
        "group.member.joined", "group.member.left",
        "approval.vote.cast", "approval.resolved",
        "task.transitioned",
      ];
      for (const eventType of eventTypes) {
        es.addEventListener(eventType, ((event: MessageEvent) => {
          try {
            const data = JSON.parse(event.data) as GroupEvent;
            dispatchEvent(data, handlersRef.current);
          } catch { /* ignore */ }
        }) as EventListener);
      }

      es.onerror = () => {
        setConnected(false);
        handlersRef.current.onDisconnect?.();
      };
    }

    function startWS() {
      if (cleaned) return;
      const ws = new WebSocket(wsUrl);
      wsRef.current = ws;

      ws.onopen = () => {
        setConnected(true);
        setTransport("ws");
        handlersRef.current.onConnect?.();
        for (const groupId of subscribedRef.current) {
          ws.send(JSON.stringify({ type: "subscribe", group_id: groupId }));
        }
      };

      ws.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data) as GroupEvent;
          if (data.type === "subscribed") return;
          dispatchEvent(data, handlersRef.current);
        } catch { /* ignore non-JSON */ }
      };

      ws.onclose = () => {
        if (!wsFailed) {
          setConnected(false);
          handlersRef.current.onDisconnect?.();
        }
      };

      ws.onerror = () => {
        wsFailed = true;
        ws.close();
        startSSE();
      };

      // Fallback to SSE if WS doesn't open within 3s
      setTimeout(() => {
        if (!wsFailed && ws.readyState !== WebSocket.OPEN) {
          wsFailed = true;
          ws.close();
          startSSE();
        }
      }, 3000);
    }

    startWS();

    const pingInterval = setInterval(() => {
      const ws = wsRef.current;
      if (ws?.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: "ping" }));
      }
    }, 30000);

    return () => {
      cleaned = true;
      clearInterval(pingInterval);
      wsRef.current?.close();
      esRef.current?.close();
    };
  }, []);

  const subscribe = useCallback((groupId: string) => {
    subscribedRef.current.add(groupId);
    const ws = wsRef.current;
    if (ws?.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ type: "subscribe", group_id: groupId }));
    }
  }, []);

  const unsubscribe = useCallback((groupId: string) => {
    subscribedRef.current.delete(groupId);
    const ws = wsRef.current;
    if (ws?.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ type: "unsubscribe", group_id: groupId }));
    }
  }, []);

  return { connected, transport, subscribe, unsubscribe };
}

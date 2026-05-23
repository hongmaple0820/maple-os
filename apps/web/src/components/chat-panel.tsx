"use client";

import { useState, useRef, useEffect } from "react";
import type { ChatMessage, ToolCall } from "@/lib/types";
import { Button, Input, Badge, Card, CardContent } from "@mapleos/ui";

function MessageBubble({ message }: { message: ChatMessage }) {
  const isUser = message.role === "user";
  const isSystem = message.role === "system";
  const isTool = message.role === "tool";

  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"} mb-4`}>
      <div
        className={`max-w-[80%] rounded-lg px-4 py-2 ${
          isUser
            ? "bg-primary text-primary-foreground"
            : isSystem
            ? "bg-muted text-muted-foreground"
            : isTool
            ? "bg-secondary text-secondary-foreground"
            : "bg-card border"
        }`}
      >
        {!isUser && (
          <div className="flex items-center gap-2 mb-1">
            <Badge variant="outline" className="text-xs">
              {message.role}
            </Badge>
            <span className="text-xs opacity-60">
              {new Date(message.timestamp).toLocaleTimeString()}
            </span>
          </div>
        )}
        <div className="text-sm whitespace-pre-wrap">{message.content}</div>
        {message.toolCalls && message.toolCalls.length > 0 && (
          <div className="mt-2 space-y-1">
            {message.toolCalls.map((tc: ToolCall) => (
              <ToolCallCard key={tc.id} toolCall={tc} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function ToolCallCard({ toolCall }: { toolCall: ToolCall }) {
  const statusVariant = {
    pending: "outline",
    running: "secondary",
    completed: "default",
    failed: "destructive",
  } as const;

  return (
    <Card className="border-dashed">
      <CardContent className="p-2">
        <div className="flex items-center gap-2">
          <Badge variant={statusVariant[toolCall.status]} className="text-xs">
            {toolCall.status}
          </Badge>
          <span className="text-xs font-medium">{toolCall.name}</span>
        </div>
        {toolCall.status === "completed" && toolCall.result !== undefined && (
          <pre className="mt-1 text-xs bg-muted p-1 rounded overflow-x-auto">
            {JSON.stringify(toolCall.result as object, null, 2)}
          </pre>
        )}
      </CardContent>
    </Card>
  );
}

export function ChatPanel() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [isStreaming, setIsStreaming] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    scrollRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const sendMessage = async () => {
    if (!input.trim() || isStreaming) return;

    const userMessage: ChatMessage = {
      id: `msg-${Date.now()}`,
      role: "user",
      content: input.trim(),
      timestamp: Date.now(),
    };

    setMessages((prev) => [...prev, userMessage]);
    setInput("");
    setIsStreaming(true);

    const assistantMessage: ChatMessage = {
      id: `msg-${Date.now() + 1}`,
      role: "assistant",
      content: "",
      timestamp: Date.now(),
      toolCalls: [],
    };

    setMessages((prev) => [...prev, assistantMessage]);

    try {
      const res = await fetch(`${process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:7788"}/api/chat`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ message: userMessage.content, history: messages }),
      });

      if (!res.ok) throw new Error(`Chat error: ${res.status}`);

      const reader = res.body?.getReader();
      const decoder = new TextDecoder();

      if (reader) {
        let buffer = "";
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          buffer += decoder.decode(value, { stream: true });

          const lines = buffer.split("\n");
          buffer = lines.pop() ?? "";

          for (const line of lines) {
            if (!line.startsWith("data: ")) continue;
            const payload = line.slice(6);
            if (payload === "[DONE]") continue;

            try {
              const event = JSON.parse(payload);
              if (event.type === "content") {
                setMessages((prev) => {
                  const updated = [...prev];
                  const last = updated[updated.length - 1];
                  if (last.role === "assistant") {
                    last.content += event.text ?? "";
                  }
                  return updated;
                });
              } else if (event.type === "tool_call") {
                setMessages((prev) => {
                  const updated = [...prev];
                  const last = updated[updated.length - 1];
                  if (last.role === "assistant" && last.toolCalls) {
                    last.toolCalls.push({
                      id: event.id ?? `tc-${Date.now()}`,
                      name: event.name ?? "",
                      arguments: event.arguments ?? {},
                      status: event.status ?? "running",
                    });
                  }
                  return updated;
                });
              } else if (event.type === "tool_result") {
                setMessages((prev) => {
                  const updated = [...prev];
                  const last = updated[updated.length - 1];
                  if (last.role === "assistant" && last.toolCalls) {
                    const tc = last.toolCalls.find((t) => t.id === event.tool_call_id);
                    if (tc) {
                      tc.result = event.result;
                      tc.status = "completed";
                    }
                  }
                  return updated;
                });
              }
            } catch {
              // skip malformed SSE lines
            }
          }
        }
      }
    } catch (err) {
      setMessages((prev) => {
        const updated = [...prev];
        const last = updated[updated.length - 1];
        if (last.role === "assistant") {
          last.content = `Error: ${(err as Error).message}`;
        }
        return updated;
      });
    } finally {
      setIsStreaming(false);
    }
  };

  return (
    <div className="flex flex-col h-full">
      {/* Messages */}
      <div className="flex-1 overflow-y-auto p-4">
        {messages.length === 0 && (
          <div className="text-center text-muted-foreground text-sm mt-8">
            Start a conversation with MapleOS
          </div>
        )}
        {messages.map((msg) => (
          <MessageBubble key={msg.id} message={msg} />
        ))}
        <div ref={scrollRef} />
      </div>

      {/* Input */}
      <div className="border-t p-4">
        <div className="flex gap-2 max-w-3xl mx-auto">
          <Input
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                sendMessage();
              }
            }}
            placeholder="Send a message..."
            disabled={isStreaming}
          />
          <Button onClick={sendMessage} disabled={isStreaming || !input.trim()}>
            {isStreaming ? "..." : "Send"}
          </Button>
        </div>
      </div>
    </div>
  );
}
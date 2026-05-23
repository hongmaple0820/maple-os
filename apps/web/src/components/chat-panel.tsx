"use client";

import { useState, useRef, useEffect } from "react";
import type { ChatMessage, ToolCall } from "@/lib/types";
import { Button, Input, Badge, Card, CardContent } from "@mapleos/ui";
import { mapleApi } from "@/lib/api";

function MessageBubble({ message }: { message: ChatMessage }) {
  const isUser = message.role === "user";
  const isSystem = message.role === "system";
  const isTool = message.role === "tool";

  const roleLabel: Record<string, string> = {
    user: "你",
    assistant: "助手",
    system: "系统",
    tool: "工具",
  };

  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"} mb-4`}>
      <div
        className={`max-w-[80%] rounded-lg px-4 py-2 ${
          isUser ? "bg-primary text-primary-foreground"
            : isSystem ? "bg-muted text-muted-foreground"
            : isTool ? "bg-secondary text-secondary-foreground"
            : "bg-card border"
        }`}
      >
        {!isUser && (
          <div className="flex items-center gap-2 mb-1">
            <Badge variant="outline" className="text-xs">{roleLabel[message.role] ?? message.role}</Badge>
            <span className="text-xs opacity-60">{new Date(message.timestamp).toLocaleTimeString("zh-CN")}</span>
          </div>
        )}
        <div className="text-sm whitespace-pre-wrap">{message.content}</div>
        {message.toolCalls && message.toolCalls.length > 0 && (
          <div className="mt-2 space-y-1">
            {message.toolCalls.map((tc: ToolCall) => <ToolCallCard key={tc.id} toolCall={tc} />)}
          </div>
        )}
      </div>
    </div>
  );
}

function ToolCallCard({ toolCall }: { toolCall: ToolCall }) {
  const statusLabel: Record<string, string> = { pending: "等待中", running: "执行中", completed: "已完成", failed: "失败" };
  const statusVariant: Record<string, "outline" | "secondary" | "default" | "destructive"> = { pending: "outline", running: "secondary", completed: "default", failed: "destructive" };

  return (
    <Card className="border-dashed">
      <CardContent className="p-2">
        <div className="flex items-center gap-2">
          <Badge variant={statusVariant[toolCall.status]} className="text-xs">{statusLabel[toolCall.status] ?? toolCall.status}</Badge>
          <span className="text-xs font-medium">{toolCall.name}</span>
        </div>
        {toolCall.status === "completed" && toolCall.result !== undefined && (
          <pre className="mt-1 text-xs bg-muted p-1 rounded overflow-x-auto">{JSON.stringify(toolCall.result as object, null, 2)}</pre>
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

  useEffect(() => { scrollRef.current?.scrollIntoView({ behavior: "smooth" }); }, [messages]);

  const sendMessage = async () => {
    if (!input.trim() || isStreaming) return;
    const userMsg: ChatMessage = { id: `msg-${Date.now()}`, role: "user", content: input.trim(), timestamp: Date.now() };
    setMessages((prev) => [...prev, userMsg]);
    setInput("");
    setIsStreaming(true);
    const assistantMsg: ChatMessage = { id: `msg-${Date.now() + 1}`, role: "assistant", content: "", timestamp: Date.now(), toolCalls: [] };
    setMessages((prev) => [...prev, assistantMsg]);

    try {
      const res = await fetch(`/api/maple/api/chat`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ message: userMsg.content }),
      });
      if (!res.ok) throw new Error(`请求失败: ${res.status}`);

      const data = await res.json() as { reply: string; session_id: string; tool_calls?: unknown[] };
      setMessages((prev) => {
        const updated = [...prev];
        const last = updated[updated.length - 1];
        if (last.role === "assistant") {
          last.content = data.reply ?? "";
        }
        return updated;
      });
    } catch (err) {
      setMessages((prev) => {
        const updated = [...prev];
        const last = updated[updated.length - 1];
        if (last.role === "assistant") last.content = `错误: ${(err as Error).message}`;
        return updated;
      });
    } finally {
      setIsStreaming(false);
    }
  };

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 overflow-y-auto p-4">
        {messages.length === 0 && <div className="text-center text-muted-foreground text-sm mt-8">开始与 MapleOS 助手对话</div>}
        {messages.map((msg) => <MessageBubble key={msg.id} message={msg} />)}
        <div ref={scrollRef} />
      </div>
      <div className="border-t p-4">
        <div className="flex gap-2 max-w-3xl mx-auto">
          <Input value={input} onChange={(e) => setInput(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); sendMessage(); } }} placeholder="输入消息..." disabled={isStreaming} />
          <Button onClick={sendMessage} disabled={isStreaming || !input.trim()}>{isStreaming ? "发送中..." : "发送"}</Button>
        </div>
      </div>
    </div>
  );
}
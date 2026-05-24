"use client";

import { useState, useRef, useEffect } from "react";
import type { ChatMessage, ToolCall } from "@/lib/types";
import { Button, Input, Badge, Card, CardContent, Spinner } from "@mapleos/ui";
import { mapleApi, rpcCall } from "@/lib/api";

interface AgentOption { id: string; name: string }

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
        className={`max-w-[80%] rounded-lg px-4 py-2.5 ${
          isUser ? "bg-primary text-primary-foreground"
            : isSystem ? "bg-muted text-muted-foreground"
            : isTool ? "bg-secondary text-secondary-foreground"
            : "bg-card border shadow-card"
        }`}
      >
        {!isUser && (
          <div className="flex items-center gap-2 mb-1">
            <Badge variant="outline" className="text-[10px]">{roleLabel[message.role] ?? message.role}</Badge>
            <span className="text-[10px] opacity-50">{new Date(message.timestamp).toLocaleTimeString("zh-CN")}</span>
          </div>
        )}
        <div className="text-[13px] leading-snug whitespace-pre-wrap">{message.content}</div>
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
    <Card className="border-dashed shadow-none">
      <CardContent className="p-2">
        <div className="flex items-center gap-2">
          <Badge variant={statusVariant[toolCall.status]} className="text-[10px]">{statusLabel[toolCall.status] ?? toolCall.status}</Badge>
          <span className="text-[12px] font-medium">{toolCall.name}</span>
        </div>
        {toolCall.status === "completed" && toolCall.result !== undefined && (
          <pre className="mt-1 text-[11px] bg-muted p-1 rounded overflow-x-auto">{JSON.stringify(toolCall.result as object, null, 2)}</pre>
        )}
      </CardContent>
    </Card>
  );
}

const QUICK_PROMPTS = [
  { label: "帮我写一段 Rust 异步任务处理代码", prompt: "帮我写一段 Rust 异步任务处理代码，使用 Tokio runtime" },
  { label: "分析最近的任务执行情况", prompt: "分析最近的任务执行情况，给出优化建议" },
  { label: "解释 Workflow DAG 调度原理", prompt: "解释 MapleOS Workflow DAG 调度原理" },
  { label: "推荐一个适合代码生成的模型", prompt: "推荐一个适合代码生成场景的 LLM 模型，考虑成本和效果" },
];

export function ChatPanel() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [isStreaming, setIsStreaming] = useState(false);
  const [agents, setAgents] = useState<AgentOption[]>([]);
  const [selectedAgent, setSelectedAgent] = useState<string>("");
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const loadAgents = async () => {
      try {
        const r = await rpcCall<{ agents: AgentOption[] }>("agent.list");
        const list = r.agents ?? [];
        setAgents(list);
        if (list.length > 0 && !selectedAgent) setSelectedAgent(list[0].id);
      } catch { setAgents([]); }
    };
    loadAgents();
  }, []);

  useEffect(() => { scrollRef.current?.scrollIntoView({ behavior: "smooth" }); }, [messages]);

  const sendMessage = async (overrideInput?: string) => {
    const text = overrideInput ?? input;
    if (!text.trim() || isStreaming) return;
    const userMsg: ChatMessage = { id: `msg-${Date.now()}`, role: "user", content: text.trim(), timestamp: Date.now() };
    setMessages((prev) => [...prev, userMsg]);
    setInput("");
    setIsStreaming(true);
    const assistantMsg: ChatMessage = { id: `msg-${Date.now() + 1}`, role: "assistant", content: "", timestamp: Date.now(), toolCalls: [] };
    setMessages((prev) => [...prev, assistantMsg]);

    try {
      const res = await fetch(`/api/maple/api/chat/stream`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ message: userMsg.content, agent_id: selectedAgent }),
      });
      if (!res.ok) throw new Error(`请求失败: ${res.status}`);

      const reader = res.body?.getReader();
      if (!reader) throw new Error("无响应体");

      const decoder = new TextDecoder();
      let buffer = "";
      let accumulated = "";

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });

        const lines = buffer.split("\n");
        buffer = lines.pop() ?? "";

        for (const line of lines) {
          if (line.startsWith("event:")) continue;
          if (line.startsWith("data:")) {
            const dataStr = line.slice(5).trim();
            if (!dataStr) continue;
            try {
              const parsed = JSON.parse(dataStr);
              if (parsed.done) {
                setMessages((prev) => {
                  const updated = [...prev];
                  const last = updated[updated.length - 1];
                  if (last.role === "assistant") last.content = accumulated;
                  return updated;
                });
                break;
              }
              if (parsed.token) {
                accumulated += parsed.token;
                setMessages((prev) => {
                  const updated = [...prev];
                  const last = updated[updated.length - 1];
                  if (last.role === "assistant") last.content = accumulated;
                  return updated;
                });
              }
              if (parsed.session_id) {
                setMessages((prev) => {
                  const updated = [...prev];
                  const last = updated[updated.length - 1];
                  if (last.role === "assistant") last.content = accumulated || "思考中...";
                  return updated;
                });
              }
            } catch { /* ignore non-JSON data lines */ }
          }
        }
      }
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
      <div className="h-10 border-b bg-card flex items-center justify-between px-4">
        <div className="flex items-center gap-2">
          <h2 className="text-[15px] font-semibold">对话</h2>
          {agents.length > 0 && (
            <select
              value={selectedAgent}
              onChange={(e) => setSelectedAgent(e.target.value)}
              className="h-7 rounded border bg-background text-[12px] px-2 font-mono"
            >
              {agents.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
            </select>
          )}
        </div>
        <div className="flex items-center gap-2">
          {isStreaming && <Spinner className="w-4 h-4" />}
          <Badge variant="outline" className="text-[10px]">{messages.length} 条消息</Badge>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-4">
        {messages.length === 0 && (
          <div className="space-y-6 mt-8">
            <div className="text-center text-muted-foreground text-sm">选择 Agent，开始与 MapleOS 协作</div>
            <div className="grid grid-cols-2 gap-2 max-w-xl mx-auto">
              {QUICK_PROMPTS.map((qp) => (
                <button
                  key={qp.label}
                  onClick={() => sendMessage(qp.prompt)}
                  className="rounded-lg border p-3 text-[12px] text-left hover:bg-accent hover:shadow-card transition-all"
                >
                  {qp.label}
                </button>
              ))}
            </div>
          </div>
        )}
        {messages.map((msg) => <MessageBubble key={msg.id} message={msg} />)}
        <div ref={scrollRef} />
      </div>

      <div className="border-t bg-card p-3">
        <div className="flex gap-2 max-w-3xl mx-auto">
          <Input
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); sendMessage(); } }}
            placeholder={selectedAgent ? `与 ${agents.find((a) => a.id === selectedAgent)?.name ?? selectedAgent} 对话...` : "输入消息..."}
            disabled={isStreaming}
            className="h-8 text-xs"
          />
          <Button size="sm" onClick={() => sendMessage()} disabled={isStreaming || !input.trim()}>
            {isStreaming ? "发送中..." : "发送"}
          </Button>
        </div>
      </div>
    </div>
  );
}
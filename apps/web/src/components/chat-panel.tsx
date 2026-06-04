"use client";

import { useState, useRef, useEffect } from "react";
import type { ChatMessage, ToolCall, KnowledgeRef } from "@/lib/types";
import { Button, Input, Badge, Card, CardContent, Spinner } from "@mapleos/ui";
import { mapleApi, rpcCall, getAuthState } from "@/lib/api";
import { useTranslation } from "react-i18next";

interface AgentOption { id: string; name: string }

function MessageBubble({ message }: { message: ChatMessage }) {
  const { t, i18n } = useTranslation();
  const isUser = message.role === "user";
  const isSystem = message.role === "system";
  const isTool = message.role === "tool";

  const roleLabel: Record<string, string> = {
    user: t("chat.role.user"),
    assistant: t("chat.role.assistant"),
    system: t("chat.role.system"),
    tool: t("chat.role.tool"),
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
            <span className="text-[10px] opacity-50">{new Date(message.timestamp).toLocaleTimeString(i18n.language?.startsWith("zh") ? "zh-CN" : "en-US")}</span>
          </div>
        )}
        <div className="text-[13px] leading-snug whitespace-pre-wrap">{message.content}</div>
        {message.toolCalls && message.toolCalls.length > 0 && (
          <div className="mt-2 space-y-1">
            {message.toolCalls.map((tc: ToolCall) => <ToolCallCard key={tc.id} toolCall={tc} />)}
          </div>
        )}
        {message.knowledgeRefs && message.knowledgeRefs.length > 0 && (
          <div className="mt-2 space-y-1">
            <div className="text-[10px] text-muted-foreground">{t("chat.knowledgeRef")}</div>
            {message.knowledgeRefs.map((ref: KnowledgeRef) => <KnowledgeRefCard key={ref.id} ref={ref} />)}
          </div>
        )}
      </div>
    </div>
  );
}

function ToolCallCard({ toolCall }: { toolCall: ToolCall }) {
  const { t } = useTranslation();
  const statusLabel: Record<string, string> = { pending: t("chat.toolStatus.pending"), running: t("chat.toolStatus.running"), completed: t("chat.toolStatus.completed"), failed: t("chat.toolStatus.failed") };
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

function KnowledgeRefCard({ ref }: { ref: KnowledgeRef }) {
  const scorePercent = Math.round(ref.score * 100);
  const scoreColor = scorePercent >= 80 ? "bg-success" : scorePercent >= 50 ? "bg-primary" : "bg-warning";

  return (
    <Card className="border-dashed shadow-none">
      <CardContent className="p-2">
        <div className="flex items-center gap-2">
          <Badge variant="outline" className="text-[10px]">{ref.source_type}</Badge>
          <span className="text-[12px] font-medium truncate max-w-[200px]">{ref.title}</span>
          <div className="flex items-center gap-1 ml-auto">
            <div className={`h-1.5 rounded-full ${scoreColor}`} style={{ width: `${scorePercent}px` }} />
            <span className="text-[10px] text-muted-foreground">{scorePercent}%</span>
          </div>
        </div>
        {ref.snippet && <div className="mt-1 text-[11px] text-muted-foreground line-clamp-2">{ref.snippet}</div>}
      </CardContent>
    </Card>
  );
}

const QUICK_PROMPTS = [
  { label: "chat.quickPrompts.rustAsync", prompt: "帮我写一段 Rust 异步任务处理代码，使用 Tokio runtime" },
  { label: "chat.quickPrompts.analyzeTasks", prompt: "分析最近的任务执行情况，给出优化建议" },
  { label: "chat.quickPrompts.workflowDag", prompt: "解释 MapleOS Workflow DAG 调度原理" },
  { label: "chat.quickPrompts.recommendModel", prompt: "推荐一个适合代码生成场景的 LLM 模型，考虑成本和效果" },
];

export function ChatPanel() {
  const { t } = useTranslation();
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [isStreaming, setIsStreaming] = useState(false);
  const [isThinking, setIsThinking] = useState(false);
  const [agents, setAgents] = useState<AgentOption[]>([]);
  const [selectedAgent, setSelectedAgent] = useState<string>("");
  const [currentSession, setCurrentSession] = useState<string>("");
  const [sessionList, setSessionList] = useState<{ id: string; title: string; created_at: number }[]>([]);
  const [models, setModels] = useState<string[]>([]);
  const [selectedModel, setSelectedModel] = useState<string>("auto");
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
    const loadModels = async () => {
      try {
        const r = await mapleApi<{ models: string[] }>("/api/models");
        setModels(r.models ?? []);
      } catch { setModels([]); }
    };
    loadAgents();
    loadModels();
    loadSessions();
  }, []);

  const loadSessions = async () => {
    try {
      const res = await mapleApi<{ sessions: { id: string; title: string; created_at: number }[] }>("/api/sessions");
      setSessionList(res.sessions ?? []);
    } catch { setSessionList([]); }
  };

  const newSession = () => {
    setMessages([]);
    setCurrentSession("");
  };

  const loadSessionMessages = async (sessionId: string) => {
    if (!sessionId) { setMessages([]); return; }
    try {
      const res = await mapleApi<{ messages: { role: string; content: string; created_at: number }[] }>(
        `/api/sessions/${sessionId}/messages`
      );
      const loaded: ChatMessage[] = (res.messages ?? []).map((m, i) => ({
        id: `loaded-${i}`,
        role: m.role as "user" | "assistant",
        content: m.content,
        timestamp: m.created_at * 1000,
      }));
      setMessages(loaded);
    } catch {
      setMessages([]);
    }
  };

  useEffect(() => { scrollRef.current?.scrollIntoView({ behavior: "smooth" }); }, [messages]);

  const sendMessage = async (overrideInput?: string) => {
    const text = overrideInput ?? input;
    if (!text.trim() || isStreaming) return;
    const sessionId = currentSession || `session-${Date.now()}`;
    if (!currentSession) setCurrentSession(sessionId);
    const userMsg: ChatMessage = { id: `msg-${Date.now()}`, role: "user", content: text.trim(), timestamp: Date.now() };
    setMessages((prev) => [...prev, userMsg]);
    setInput("");
    setIsStreaming(true);

    let knowledgeRefs: KnowledgeRef[] = [];
    try {
      const kbRes = await mapleApi<{ results: KnowledgeRef[] }>("/api/kb/search", {
        method: "POST",
        body: { query: text.trim(), top_k: 3 },
      });
      knowledgeRefs = kbRes.results ?? [];
    } catch { /* kb search optional */ }

    const assistantMsg: ChatMessage = { id: `msg-${Date.now() + 1}`, role: "assistant", content: "", timestamp: Date.now(), toolCalls: [], knowledgeRefs };
    setMessages((prev) => [...prev, assistantMsg]);

    try {
      const { token } = getAuthState();
      const res = await fetch(`/api/maple/api/chat/stream`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          ...(token ? { Authorization: `Bearer ${token}` } : {}),
        },
        body: JSON.stringify({ message: userMsg.content, agent_id: selectedAgent, session_id: sessionId, model: selectedModel }),
      });
      if (!res.ok) throw new Error(t("chat.error.requestFailed", { status: res.status }));

      const reader = res.body?.getReader();
      if (!reader) throw new Error(t("chat.error.noResponse"));

      const decoder = new TextDecoder();
      let buffer = "";
      let accumulated = "";
      let currentEvent = "";

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });

        const lines = buffer.split("\n");
        buffer = lines.pop() ?? "";

        for (const line of lines) {
          if (line.startsWith("event:")) {
            currentEvent = line.slice(6).trim();
            continue;
          }
          if (line.startsWith("data:")) {
            const dataStr = line.slice(5).trim();
            if (!dataStr) continue;
            if (currentEvent === "error") {
              let errorMsg = dataStr;
              try { const p = JSON.parse(dataStr); errorMsg = p.message ?? p.error ?? dataStr; } catch { /* plain text */ }
              setMessages((prev) => {
                const updated = [...prev];
                const last = updated[updated.length - 1];
                if (last.role === "assistant") updated[updated.length - 1] = { ...last, content: accumulated || t("chat.error.llmUnavailable", { error: errorMsg }) };
                return updated;
              });
              break;
            }
            if (currentEvent === "thinking") {
              setIsThinking(true);
              continue;
            }
            try {
              const parsed = JSON.parse(dataStr);
              if (parsed.done) {
                setIsThinking(false);
                setMessages((prev) => {
                  const updated = [...prev];
                  const last = updated[updated.length - 1];
                  if (last.role === "assistant") updated[updated.length - 1] = { ...last, content: accumulated };
                  return updated;
                });
                break;
              }
              if (parsed.token) {
                setIsThinking(false);
                accumulated += parsed.token;
                setMessages((prev) => {
                  const updated = [...prev];
                  const last = updated[updated.length - 1];
                  if (last.role === "assistant") updated[updated.length - 1] = { ...last, content: accumulated };
                  return updated;
                });
              }
              if (parsed.session_id && parsed.model) {
                setCurrentSession(parsed.session_id);
              }
            } catch { /* ignore non-JSON data lines */ }
          }
        }
      }
    } catch (err) {
      setMessages((prev) => {
        const updated = [...prev];
        const last = updated[updated.length - 1];
        if (last.role === "assistant") last.content = `${t("common.failed")}: ${(err as Error).message}`;
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
          <h2 className="text-[15px] font-semibold">{t("chat.title")}</h2>
          {agents.length > 0 && (
            <select
              value={selectedAgent}
              onChange={(e) => setSelectedAgent(e.target.value)}
              className="h-7 rounded border bg-background text-[12px] px-2 font-mono"
            >
              {agents.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
            </select>
          )}
          {models.length > 0 && (
            <select
              value={selectedModel}
              onChange={(e) => setSelectedModel(e.target.value)}
              className="h-7 rounded border bg-background text-[12px] px-2 font-mono"
            >
              <option value="auto">{t("chat.autoSelect")}</option>
              {models.map((m) => <option key={m} value={m}>{m}</option>)}
            </select>
          )}
        </div>
        <div className="flex items-center gap-2">
          {isThinking && <span className="text-[11px] text-muted-foreground animate-pulse">{t("chat.thinking")}</span>}
          {isStreaming && !isThinking && <Spinner className="w-4 h-4" />}
          <Badge variant="outline" className="text-[10px]">{t("chat.messageCount", { count: messages.length })}</Badge>
          {sessionList.length > 1 && (
            <select
              value={currentSession}
              onChange={(e) => { const sid = e.target.value; setCurrentSession(sid); loadSessionMessages(sid); }}
              className="h-7 rounded border bg-background text-[11px] px-1"
            >
              <option value="">{t("chat.newSession")}</option>
              {sessionList.map((s) => <option key={s.id} value={s.id}>{s.title}</option>)}
            </select>
          )}
          <Button size="sm" variant="ghost" onClick={newSession}>{t("chat.newSession")}</Button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-4">
        {messages.length === 0 && (
          <div className="space-y-6 mt-8">
            <div className="text-center text-muted-foreground text-sm">{t("chat.emptyState")}</div>
            <div className="grid grid-cols-2 gap-2 max-w-xl mx-auto">
              {QUICK_PROMPTS.map((qp) => (
                <button
                  key={qp.label}
                  onClick={() => sendMessage(qp.prompt)}
                  className="rounded-lg border p-3 text-[12px] text-left hover:bg-accent hover:shadow-card transition-all"
                >
                  {t(qp.label)}
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
            placeholder={selectedAgent ? t("chat.placeholder.withAgent", { name: agents.find((a) => a.id === selectedAgent)?.name ?? selectedAgent }) : t("chat.placeholder.default")}
            disabled={isStreaming}
            className="h-8 text-xs"
          />
          <Button size="sm" onClick={() => sendMessage()} disabled={isStreaming || !input.trim()}>
            {isStreaming ? t("chat.sending") : t("chat.send")}
          </Button>
        </div>
      </div>
    </div>
  );
}
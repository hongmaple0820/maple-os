export const API_BASE_URL = process.env.NEXT_PUBLIC_MAPLE_API_URL || "";
export const API_PORT = 7788;
export const SCALE_PORT = 7790;

export const ROUTES = {
  SYSTEM_INFO: "/api/maple/api/system/info",
  HEALTH: "/api/maple/health",
  CHAT: "/api/maple/api/chat",
  CHAT_STREAM: "/api/maple/api/chat/stream",
  SESSIONS: "/api/maple/api/sessions",
  WORKFLOWS: "/api/maple/api/workflows",
  KB_SEARCH: "/api/maple/api/kb/search",
  KB_INDEX: "/api/maple/api/kb/index",
  MEMORIES: "/api/maple/api/memories",
  MEMORIES_SEARCH: "/api/maple/api/memories/search",
  EVENTS: "/api/maple/api/events",
  RPC: "/rpc",
  SCALE: "/api/scale",
};

export const DEFAULT_MODELS = {
  OLLAMA_DEFAULT: "ollama/qwen2.5:7b",
  OPENAI_DEFAULT: "openai/gpt-4o-mini",
  CLAUDE_DEFAULT: "anthropic/claude-3-5-sonnet",
  DEEPSEEK_DEFAULT: "deepseek-chat",
};

export const THEME = {
  COLORS: {
    primary: "#6366f1",
    success: "#22c55e",
    warning: "#f59e0b",
    destructive: "#ef4444",
    info: "#4fc3f7",
    background: "#0f0f23",
    card: "#1a1a2e",
    muted: "#888888",
  },
  FONT: {
    family: "Inter, system-ui, sans-serif",
    mono: "JetBrains Mono, monospace",
    size: {
      xs: "10px",
      sm: "11px",
      base: "13px",
      md: "14px",
      lg: "16px",
      xl: "20px",
    },
  },
};

export const LIMITS = {
  CHAT_MAX_TOKENS: 4096,
  KB_SEARCH_TOP_K: 8,
  KB_MAX_PAGES: 200,
  CODE_EXEC_TIMEOUT_SECS: 10,
  CODE_EXEC_MAX_OUTPUT_BYTES: 8192,
  SCHEDULER_INTERVAL_SECS: 60,
  TASK_WORKER_INTERVAL_SECS: 2,
};
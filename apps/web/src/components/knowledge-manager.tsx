"use client";

import { useState } from "react";
import { Card, CardContent, Badge, Button, Input, Spinner } from "@mapleos/ui";
import { mapleApi } from "@/lib/api";

interface KbSearchResult {
  results: Array<{ id: string; content: string; score: number; metadata: Record<string, unknown>; source_type?: string }>;
}

interface IndexLog { id: string; title: string; source_type: string; timestamp: number }

const SOURCE_TYPES = [
  { value: "document", label: "文档" },
  { value: "code", label: "代码" },
  { value: "conversation", label: "对话" },
  { value: "web", label: "网页" },
  { value: "note", label: "笔记" },
];

export function KnowledgeManager() {
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<KbSearchResult | null>(null);
  const [searching, setSearching] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [uploadTitle, setUploadTitle] = useState("");
  const [uploadText, setUploadText] = useState("");
  const [uploadSource, setUploadSource] = useState("document");
  const [showUpload, setShowUpload] = useState(false);
  const [indexLogs, setIndexLogs] = useState<IndexLog[]>([]);
  const [activeTab, setActiveTab] = useState<"search" | "index" | "recent">("search");

  const handleSearch = async () => {
    if (!searchQuery.trim()) return;
    setSearching(true);
    try {
      const result = await mapleApi<KbSearchResult>("/api/kb/search", { method: "POST", body: { query: searchQuery.trim(), top_k: 8 } });
      setSearchResults(result);
    } catch (err) { alert(`搜索失败: ${(err as Error).message}`); }
    finally { setSearching(false); }
  };

  const handleIndex = async () => {
    if (!uploadText.trim()) return;
    setUploading(true);
    try {
      await mapleApi("/api/kb/index", {
        method: "POST",
        body: { title: uploadTitle.trim() || "未命名文档", content: uploadText.trim(), source_type: uploadSource },
      });
      const log: IndexLog = { id: `idx-${Date.now()}`, title: uploadTitle.trim() || "未命名文档", source_type: uploadSource, timestamp: Date.now() };
      setIndexLogs((prev) => [log, ...prev]);
      setShowUpload(false); setUploadTitle(""); setUploadText(""); setUploadSource("document");
    } catch (err) { alert(`索引失败: ${(err as Error).message}`); }
    finally { setUploading(false); }
  };

  const scoreColor = (score: number) => {
    if (score >= 0.8) return "text-success";
    if (score >= 0.5) return "text-primary";
    if (score >= 0.3) return "text-warning";
    return "text-muted-foreground";
  };

  const sourceLabel = (source: string) => {
    const found = SOURCE_TYPES.find((s) => s.value === source);
    return found?.label ?? source;
  };

  return (
    <div className="flex flex-col h-full">
      <div className="h-10 border-b bg-card flex items-center justify-between px-4">
        <h2 className="text-[15px] font-semibold">知识库</h2>
        <div className="flex items-center gap-2">
          <Input value={searchQuery} onChange={(e) => setSearchQuery(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") handleSearch(); }} placeholder="搜索知识库..." className="w-48 h-7 text-xs" />
          <Button size="sm" onClick={handleSearch} disabled={searching || !searchQuery.trim()}>{searching ? <Spinner className="w-4 h-4" /> : "搜索"}</Button>
          <Button size="sm" onClick={() => setShowUpload(true)}>上传</Button>
        </div>
      </div>

      {showUpload && (
        <div className="border-b bg-muted/50 p-3 space-y-2">
          <div className="flex gap-2">
            <Input value={uploadTitle} onChange={(e) => setUploadTitle(e.target.value)} placeholder="文档标题..." className="w-36 h-7 text-xs" />
            <select value={uploadSource} onChange={(e) => setUploadSource(e.target.value)} className="h-7 rounded border bg-background text-xs px-2">
              {SOURCE_TYPES.map((s) => <option key={s.value} value={s.value}>{s.label}</option>)}
            </select>
          </div>
          <textarea value={uploadText} onChange={(e) => setUploadText(e.target.value)} placeholder="输入要索引的文本内容..." className="w-full h-20 rounded-md border bg-transparent px-3 py-2 text-xs placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring" />
          <div className="flex gap-2">
            <Button size="sm" onClick={handleIndex} disabled={uploading || !uploadText.trim()}>{uploading ? "索引中..." : "提交索引"}</Button>
            <Button size="sm" variant="ghost" onClick={() => { setShowUpload(false); setUploadTitle(""); setUploadText(""); }}>取消</Button>
          </div>
        </div>
      )}

      <div className="h-8 border-b bg-muted/30 flex items-center gap-2 px-4">
        {(["search", "index", "recent"] as const).map((tab) => (
          <button
            key={tab}
            onClick={() => setActiveTab(tab)}
            className={`px-2 py-1 rounded text-[11px] transition-colors ${
              activeTab === tab ? "bg-primary text-primary-foreground font-medium" : "text-muted-foreground hover:bg-accent"
            }`}
          >
            {tab === "search" ? "搜索结果" : tab === "index" ? "索引文档" : "最近索引"}
          </button>
        ))}
      </div>

      <div className="flex-1 overflow-y-auto p-4 space-y-2">
        {activeTab === "search" && (
          <>
            {searchResults === null && <div className="text-center text-muted-foreground py-8">输入关键词搜索知识库</div>}
            {searchResults && searchResults.results.length === 0 && <div className="text-center text-muted-foreground py-8">未找到相关内容</div>}
            {searchResults && searchResults.results.map((r) => (
              <Card key={r.id} className="shadow-card">
                <CardContent className="p-3">
                  <div className="flex items-center justify-between mb-1">
                    <div className="flex items-center gap-1.5">
                      <Badge variant="outline" className="text-[10px] font-mono">{r.id.slice(0, 8)}</Badge>
                      {r.source_type && <Badge variant="secondary" className="text-[10px]">{sourceLabel(r.source_type)}</Badge>}
                    </div>
                    <div className="flex items-center gap-1.5">
                      <div className={`text-[12px] font-semibold ${scoreColor(r.score)}`}>{(r.score * 100).toFixed(1)}%</div>
                      <div className="w-16 h-2 rounded-full bg-muted overflow-hidden">
                        <div className={`h-full rounded-full ${r.score >= 0.8 ? "bg-success" : r.score >= 0.5 ? "bg-primary" : r.score >= 0.3 ? "bg-warning" : "bg-muted-foreground"}`} style={{ width: `${r.score * 100}%` }} />
                      </div>
                    </div>
                  </div>
                  <div className="text-[12px] text-muted-foreground line-clamp-3">{r.content}</div>
                </CardContent>
              </Card>
            ))}
          </>
        )}

        {activeTab === "index" && (
          <div className="space-y-3">
            <div className="text-center text-muted-foreground py-4">点击上方"上传"按钮添加文档到知识库</div>
            <Card className="shadow-card">
              <CardContent className="p-4 space-y-2">
                <div className="text-[13px] font-medium">支持的文档类型</div>
                <div className="flex flex-wrap gap-1.5">
                  {SOURCE_TYPES.map((s) => <Badge key={s.value} variant="outline" className="text-[11px]">{s.label} ({s.value})</Badge>)}
                </div>
                <div className="text-[11px] text-muted-foreground">索引格式: title + content + source_type，后端使用 BM25 + Embedding 混合检索</div>
              </CardContent>
            </Card>
          </div>
        )}

        {activeTab === "recent" && (
          <>
            {indexLogs.length === 0 && <div className="text-center text-muted-foreground py-8">暂无索引记录</div>}
            {indexLogs.map((log) => (
              <Card key={log.id} className="shadow-card">
                <CardContent className="p-3">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-1.5">
                      <span className="text-[13px] font-medium">{log.title}</span>
                      <Badge variant="secondary" className="text-[10px]">{sourceLabel(log.source_type)}</Badge>
                    </div>
                    <span className="text-[10px] text-muted-foreground font-mono">{new Date(log.timestamp).toLocaleTimeString("zh-CN")}</span>
                  </div>
                </CardContent>
              </Card>
            ))}
          </>
        )}
      </div>
    </div>
  );
}
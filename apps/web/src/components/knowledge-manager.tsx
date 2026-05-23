"use client";

import { useState } from "react";
import { Card, CardHeader, CardTitle, CardContent, Badge, Button, Input, Spinner } from "@mapleos/ui";
import { mapleApi } from "@/lib/api";

interface KbSearchResult {
  results: Array<{ id: string; content: string; score: number; metadata: Record<string, unknown> }>;
}

export function KnowledgeManager() {
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<KbSearchResult | null>(null);
  const [searching, setSearching] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [uploadText, setUploadText] = useState("");
  const [showUpload, setShowUpload] = useState(false);

  const handleSearch = async () => {
    if (!searchQuery.trim()) return;
    setSearching(true);
    try {
      const result = await mapleApi<KbSearchResult>("/api/kb/search", { method: "POST", body: { query: searchQuery.trim(), top_k: 5 } });
      setSearchResults(result);
    } catch (err) { alert(`搜索失败: ${(err as Error).message}`); }
    finally { setSearching(false); }
  };

  const handleIndex = async () => {
    if (!uploadText.trim()) return;
    setUploading(true);
    try {
      await mapleApi("/api/kb/index", { method: "POST", body: { content: uploadText.trim(), metadata: { source: "web-ui" } } });
      setShowUpload(false); setUploadText(""); alert("文档已提交索引!");
    } catch (err) { alert(`索引失败: ${(err as Error).message}`); }
    finally { setUploading(false); }
  };

  return (
    <div className="flex flex-col h-full">
      <div className="border-b p-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold">知识库</h2>
        <div className="flex gap-2">
          <Input value={searchQuery} onChange={(e) => setSearchQuery(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") handleSearch(); }} placeholder="搜索知识库内容..." className="w-64" />
          <Button onClick={handleSearch} disabled={searching || !searchQuery.trim()}>{searching ? <Spinner className="w-4 h-4" /> : "搜索"}</Button>
          <Button onClick={() => setShowUpload(true)}>上传文档</Button>
        </div>
      </div>
      {showUpload && (
        <div className="border-b p-4 bg-muted/50 space-y-2">
          <textarea value={uploadText} onChange={(e) => setUploadText(e.target.value)} placeholder="输入要索引的文本内容..." className="w-full h-24 rounded-md border border-input bg-transparent px-3 py-2 text-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring" />
          <div className="flex gap-2">
            <Button onClick={handleIndex} disabled={uploading || !uploadText.trim()}>{uploading ? "索引中..." : "提交索引"}</Button>
            <Button variant="outline" onClick={() => { setShowUpload(false); setUploadText(""); }}>取消</Button>
          </div>
        </div>
      )}
      <div className="flex-1 overflow-y-auto p-4 space-y-3">
        {searchResults === null && <div className="text-center text-muted-foreground py-8">在上方输入关键词搜索知识库，或上传文档进行索引</div>}
        {searchResults && searchResults.results.length === 0 && <div className="text-center text-muted-foreground py-8">未找到相关内容</div>}
        {searchResults && searchResults.results.map((r) => (
          <Card key={r.id} className="hover:shadow-md transition-shadow">
            <CardHeader className="pb-2"><div className="flex items-center justify-between"><CardTitle className="text-sm">{r.id}</CardTitle><Badge variant="outline" className="text-xs">相似度 {(r.score * 100).toFixed(1)}%</Badge></div></CardHeader>
            <CardContent><p className="text-sm text-muted-foreground whitespace-pre-wrap line-clamp-4">{r.content}</p></CardContent>
          </Card>
        ))}
      </div>
    </div>
  );
}
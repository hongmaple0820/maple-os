import { useState } from "react";
import { StyleSheet, View, Text, TextInput, FlatList, TouchableOpacity } from "react-native";
import { mobileRpcCall } from "../lib/api";

interface KbResult { id: string; content: string; score: number; source_type: string }

export function KnowledgeScreen() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<KbResult[]>([]);
  const [searching, setSearching] = useState(false);
  const [tab, setTab] = useState<"search" | "index" | "recent">("search");

  const handleSearch = async () => {
    if (!query.trim()) return;
    setSearching(true);
    try {
      const res = await mobileRpcCall("knowledge.search", { query: query.trim(), top_k: 8 });
      setResults(res.results ?? []);
    } catch { setResults([]); }
    setSearching(false);
  };

  return (
    <View style={styles.container}>
      <Text style={styles.title}>知识库</Text>
      <View style={styles.tabBar}>
        {(["search", "index", "recent"] as const).map((t) => (
          <TouchableOpacity key={t} onPress={() => setTab(t)} style={styles.tab(tab === t)}>
            <Text style={styles.tabText(tab === t)}>{t === "search" ? "搜索" : t === "index" ? "索引" : "最近"}</Text>
          </TouchableOpacity>
        ))}
      </View>
      {tab === "search" && (
        <View style={styles.searchSection}>
          <View style={styles.searchBar}>
            <TextInput style={styles.searchInput} value={query} onChangeText={setQuery} placeholder="搜索知识库..." onSubmitEditing={handleSearch} />
            <TouchableOpacity onPress={handleSearch} style={styles.searchBtn}>
              <Text style={styles.searchBtnText}>{searching ? "..." : "搜索"}</Text>
            </TouchableOpacity>
          </View>
          <FlatList data={results} keyExtractor={(item) => item.id} renderItem={({ item }) => (
            <View style={styles.resultCard}>
              <Text style={styles.resultContent}>{item.content.slice(0, 100)}</Text>
              <View style={styles.resultMeta}>
                <Text style={styles.resultBadge}>{item.source_type}</Text>
                <Text style={styles.resultScore}>{(item.score * 100).toFixed(0)}%</Text>
              </View>
            </View>
          )} ListEmptyComponent={() => <Text style={styles.empty}>输入关键词搜索</Text>} />
        </View>
      )}
      {tab === "index" && <Text style={styles.empty}>索引文档功能开发中</Text>}
      {tab === "recent" && <Text style={styles.empty}>最近索引功能开发中</Text>}
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: "#0f0f23", padding: 16 },
  title: { fontSize: 20, fontWeight: "700", color: "#4fc3f7!", marginBottom: 16 },
  tabBar: { flexDirection: "row", gap: 8, marginBottom: 16 },
  tab: (active: boolean) => ({ paddingHorizontal: 16, paddingVertical: 8, borderRadius: 8, backgroundColor: active ? "#4fc3f7!" : "#2a2a4e" }),
  tabText: (active: boolean) => ({ color: active ? "#fff" : "#888", fontSize: 13 }),
  searchSection: { flex: 1 },
  searchBar: { flexDirection: "row", gap: 8, marginBottom: 12 },
  searchInput: { flex: 1, backgroundColor: "#2a2a4e", color: "#e0e0e0", borderRadius: 8, padding: 10, fontSize: 14 },
  searchBtn: { backgroundColor: "#4fc3f7!", paddingHorizontal: 16, paddingVertical: 10, borderRadius: 8 },
  searchBtnText: { color: "#fff", fontWeight: "600" },
  resultCard: { backgroundColor: "#1a1a2e", padding: 12, borderRadius: 8, marginBottom: 8 },
  resultContent: { color: "#e0e0e0", fontSize: 13 },
  resultMeta: { flexDirection: "row", justifyContent: "space-between", marginTop: 6 },
  resultBadge: { backgroundColor: "#2a2a4e", color: "#4fc3f7!", paddingHorizontal: 8, paddingVertical: 2, borderRadius: 4, fontSize: 11 },
  resultScore: { color: "#888", fontSize: 11 },
  empty: { color: "#888", textAlign: "center", marginTop: 40, fontSize: 14 },
});
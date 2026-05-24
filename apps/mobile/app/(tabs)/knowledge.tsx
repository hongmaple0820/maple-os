import { View, Text, TextInput, FlatList, StyleSheet } from "react-native";
import { useState } from "react";

interface KbResult { id: string; content: string; score: number; source_type?: string }

export default function KnowledgeScreen() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<KbResult[]>([]);
  const [searching, setSearching] = useState(false);
  const [activeTab, setActiveTab] = useState<"search" | "index">("search");

  const handleSearch = async () => {
    if (!query.trim()) return;
    setSearching(true);
    try {
      const res = await fetch("http://localhost:7788/api/kb/search", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ query: query.trim(), top_k: 8 }),
      });
      const data = await res.json();
      setResults(data.results ?? []);
    } catch { setResults([]); }
    setSearching(false);
  };

  const scoreColor = (score: number) => {
    if (score >= 0.8) return "#22c55e";
    if (score >= 0.5) return "#6366f1";
    return "#f59e0b";
  };

  const renderItem = ({ item }: { item: KbResult }) => (
    <View style={styles.resultCard}>
      <View style={styles.resultHeader}>
        <Text style={styles.resultScore}>{Math.round(item.score * 100)}%</Text>
        {item.source_type && <Text style={[styles.sourceBadge, { color: scoreColor(item.score) }]}>{item.source_type}</Text>}
      </View>
      <Text style={styles.resultContent} numberOfLines={3}>{item.content}</Text>
    </View>
  );

  return (
    <View style={styles.container}>
      <Text style={styles.title}>Knowledge Base</Text>
      <View style={styles.tabBar}>
        <Text style={[styles.tab, activeTab === "search" && styles.tabActive]} onPress={() => setActiveTab("search")}>Search</Text>
        <Text style={[styles.tab, activeTab === "index" && styles.tabActive]} onPress={() => setActiveTab("index")}>Index</Text>
      </View>
      {activeTab === "search" && (
        <View style={styles.searchSection}>
          <TextInput
            style={styles.searchInput}
            value={query}
            onChangeText={setQuery}
            placeholder="Search knowledge..."
            placeholderTextColor="#666"
            onSubmitEditing={handleSearch}
          />
          <FlatList data={results} renderItem={renderItem} keyExtractor={(r) => r.id} />
          {results.length === 0 && !searching && <Text style={styles.empty}>No results</Text>}
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: "#0f0f23", padding: 16 },
  title: { fontSize: 20, fontWeight: "700", color: "#e0e0e0", marginBottom: 12 },
  tabBar: { flexDirection: "row", gap: 16, marginBottom: 16 },
  tab: { color: "#888", fontSize: 14 },
  tabActive: { color: "#6366f1", fontWeight: "600" },
  searchSection: { flex: 1 },
  searchInput: { backgroundColor: "#1a1a2e", color: "#e0e0e0", borderRadius: 8, padding: 12, fontSize: 14, marginBottom: 12 },
  resultCard: { backgroundColor: "#1a1a2e", borderRadius: 8, padding: 12, marginBottom: 8 },
  resultHeader: { flexDirection: "row", justifyContent: "space-between" },
  resultScore: { color: "#6366f1", fontWeight: "600", fontSize: 12 },
  sourceBadge: { fontSize: 11 },
  resultContent: { color: "#e0e0e0", fontSize: 13, marginTop: 6 },
  empty: { color: "#888", textAlign: "center", marginTop: 32 },
});
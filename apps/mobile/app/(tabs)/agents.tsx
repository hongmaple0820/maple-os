import { View, Text, FlatList, StyleSheet } from "react-native";
import { useState, useEffect } from "react";

interface AgentItem { id: string; name: string; status: string; description: string }

export default function AgentsScreen() {
  const [agents, setAgents] = useState<AgentItem[]>([]);
  const [selectedAgent, setSelectedAgent] = useState<string>("");
  const [chatInput, setChatInput] = useState("");
  const [messages, setMessages] = useState<{ role: string; content: string }[]>([]);

  useEffect(() => {
    (async () => {
      try {
        const res = await fetch("http://localhost:7788/rpc", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ jsonrpc: "2.0", method: "agent.list", params: {}, id: 1 }),
        });
        const data = await res.json();
        setAgents(data.result?.agents ?? []);
      } catch {}
    })();
  }, []);

  const dispatchTask = async (agentId: string, task: string) => {
    try {
      await fetch("http://localhost:7788/rpc", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ jsonrpc: "2.0", method: "task.create", params: { task_type: "prompt", priority: 1, payload: { agent_id: agentId, prompt: task } }, id: 2 }),
      });
      setMessages((prev) => [...prev, { role: "system", content: `Task dispatched to ${agentId}` }]);
    } catch (err) {
      setMessages((prev) => [...prev, { role: "system", content: `Error: ${(err as Error).message}` }]);
    }
  };

  const renderAgent = ({ item }: { item: AgentItem }) => (
    <View style={styles.agentCard}>
      <View style={styles.agentHeader}>
        <Text style={styles.agentName}>{item.name}</Text>
        <Text style={[styles.agentStatus, item.status === "Online" ? styles.online : styles.offline]}>
          {item.status ?? "Unknown"}
        </Text>
      </View>
      <Text style={styles.agentDesc} numberOfLines={2}>{item.description ?? ""}</Text>
      <View style={styles.agentActions}>
        <Text style={styles.dispatchBtn} onPress={() => dispatchTask(item.id, chatInput || "Hello")}>Dispatch Task</Text>
        <Text style={styles.selectBtn} onPress={() => setSelectedAgent(item.id)}>
          {selectedAgent === item.id ? "Selected" : "Select"}
        </Text>
      </View>
    </View>
  );

  return (
    <View style={styles.container}>
      <Text style={styles.title}>Agent Center</Text>
      <FlatList data={agents} renderItem={renderAgent} keyExtractor={(a) => a.id} />
      {agents.length === 0 && <Text style={styles.empty}>No agents available</Text>}
      <View style={styles.collabBar}>
        <TextInput
          style={styles.collabInput}
          value={chatInput}
          onChangeText={setChatInput}
          placeholder="Collaboration message..."
          placeholderTextColor="#666"
        />
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: "#0f0f23", padding: 16 },
  title: { fontSize: 20, fontWeight: "700", color: "#e0e0e0", marginBottom: 12 },
  agentCard: { backgroundColor: "#1a1a2e", borderRadius: 12, padding: 12, marginBottom: 8 },
  agentHeader: { flexDirection: "row", justifyContent: "space-between" },
  agentName: { fontSize: 14, fontWeight: "600", color: "#e0e0e0" },
  agentStatus: { fontSize: 11 },
  online: { color: "#22c55e" },
  offline: { color: "#888" },
  agentDesc: { fontSize: 12, color: "#888", marginTop: 4 },
  agentActions: { flexDirection: "row", gap: 16, marginTop: 8 },
  dispatchBtn: { color: "#6366f1", fontSize: 12, fontWeight: "600" },
  selectBtn: { color: "#f59e0b", fontSize: 12 },
  empty: { color: "#888", textAlign: "center", marginTop: 32 },
  collabBar: { backgroundColor: "#1a1a2e", borderRadius: 8, padding: 8, marginTop: 8 },
  collabInput: { backgroundColor: "#0f0f23", color: "#e0e0e0", borderRadius: 6, padding: 8, fontSize: 14 },
});
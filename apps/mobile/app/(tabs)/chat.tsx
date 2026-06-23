import { View, Text, TextInput, FlatList, StyleSheet } from "react-native";
import { useState, useEffect } from "react";
import { mobileRpcCall, mobileSseCall } from "../../src/lib/api";

interface AgentOption { id: string; name: string }
interface Message { id: string; role: "user" | "assistant"; content: string; timestamp: number }

interface AgentListResult { agents: AgentOption[] }

export default function ChatScreen() {
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [agents, setAgents] = useState<AgentOption[]>([]);
  const [selectedAgent, setSelectedAgent] = useState("");
  const [streaming, setStreaming] = useState(false);

  useEffect(() => {
    mobileRpcCall<AgentListResult>("agent.list", {})
      .then((r) => {
        setAgents(r.agents ?? []);
        if (r.agents?.length) setSelectedAgent(r.agents[0].id);
      })
      .catch(() => {});
  }, []);

  const sendMessage = async () => {
    if (!input.trim() || streaming) return;
    const userMsg: Message = { id: `msg-${Date.now()}`, role: "user", content: input.trim(), timestamp: Date.now() };
    setMessages((prev) => [...prev, userMsg]);
    setInput("");
    setStreaming(true);

    const assistantMsg: Message = { id: `msg-${Date.now() + 1}`, role: "assistant", content: "", timestamp: Date.now() };
    setMessages((prev) => [...prev, assistantMsg]);

    await mobileSseCall(
      { message: userMsg.content, agent_id: selectedAgent },
      (token) => {
        setMessages((prev) => {
          const updated = [...prev];
          updated[updated.length - 1].content += token;
          return updated;
        });
      },
      (_done) => { setStreaming(false); },
      (error) => {
        setMessages((prev) => {
          const updated = [...prev];
          updated[updated.length - 1].content = `Error: ${error}`;
          return updated;
        });
        setStreaming(false);
      },
    );
  };

  const renderMessage = ({ item }: { item: Message }) => (
    <View style={[styles.messageBubble, item.role === "user" ? styles.userBubble : styles.assistantBubble]}>
      <Text style={item.role === "user" ? styles.userText : styles.assistantText}>{item.content}</Text>
    </View>
  );

  return (
    <View style={styles.container}>
      {agents.length > 0 && (
        <View style={styles.agentBar}>
          <Text style={styles.agentLabel}>Agent:</Text>
          {agents.map((a) => (
            <Text
              key={a.id}
              style={[styles.agentName, selectedAgent === a.id && styles.agentActive]}
              onPress={() => setSelectedAgent(a.id)}
            >
              {a.name}
            </Text>
          ))}
        </View>
      )}
      <FlatList data={messages} renderItem={renderMessage} keyExtractor={(m) => m.id} style={styles.messageList} />
      <View style={styles.inputBar}>
        <TextInput
          style={styles.input}
          value={input}
          onChangeText={setInput}
          placeholder="输入消息..."
          placeholderTextColor="#666"
          onSubmitEditing={sendMessage}
          editable={!streaming}
        />
        <Text style={[styles.sendBtn, streaming && styles.sendDisabled]} onPress={sendMessage}>
          {streaming ? "..." : "Send"}
        </Text>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: "#0f0f23" },
  agentBar: { flexDirection: "row", alignItems: "center", padding: 8, backgroundColor: "#1a1a2e", gap: 8 },
  agentLabel: { color: "#888", fontSize: 12 },
  agentName: { color: "#888", fontSize: 12, paddingHorizontal: 8 },
  agentActive: { color: "#6366f1", fontWeight: "600" },
  messageList: { flex: 1 },
  messageBubble: { margin: 8, padding: 12, borderRadius: 12, maxWidth: "80%" },
  userBubble: { alignSelf: "flex-end", backgroundColor: "#6366f1" },
  assistantBubble: { alignSelf: "flex-start", backgroundColor: "#1a1a2e" },
  userText: { color: "#fff", fontSize: 14 },
  assistantText: { color: "#e0e0e0", fontSize: 14 },
  inputBar: { flexDirection: "row", padding: 8, backgroundColor: "#1a1a2e", borderTopWidth: 1, borderTopColor: "#2a2a4a" },
  input: { flex: 1, backgroundColor: "#0f0f23", color: "#e0e0e0", borderRadius: 8, padding: 10, fontSize: 14 },
  sendBtn: { color: "#6366f1", fontWeight: "700", paddingHorizontal: 16, fontSize: 14, lineHeight: 40 },
  sendDisabled: { color: "#666" },
});
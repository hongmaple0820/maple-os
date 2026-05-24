import { useState, useRef } from "react";
import { StyleSheet, FlatList, TextInput, Text, View } from "react-native";
import { mobileRpcCall } from "../lib/api";

import type { ChatMessage } from "@mapleos/sdk";

interface AgentOption { id: string; name: string }

export function ChatScreen() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [agents, setAgents] = useState<AgentOption[]>([]);
  const [selectedAgent, setSelectedAgent] = useState<string>("");
  const [sending, setSending] = useState(false);

  const loadAgents = async () => {
    try {
      const result = await mobileRpcCall("agent.list", {});
      setAgents(result.agents ?? []);
      if (result.agents?.length > 0) setSelectedAgent(result.agents[0].id);
    } catch {}
  };

  useState(() => { loadAgents(); }, []);

  const sendMessage = async () => {
    if (!input.trim() || sending) return;
    const userMsg: ChatMessage = { id: `msg-${Date.now()}`, role: "user", content: input.trim(), timestamp: Date.now() };
    setMessages((prev) => [...prev, userMsg]);
    setInput("");
    setSending(true);

    try {
      const res = await mobileRpcCall("agent.chat", { message: userMsg.content, agent_id: selectedAgent });
      setMessages((prev) => [...prev, { id: `msg-${Date.now() + 1}`, role: "assistant", content: res.reply ?? "", timestamp: Date.now() }]);
    } catch {
      setMessages((prev) => [...prev, { id: `msg-${Date.now() + 1}`, role: "assistant", content: "请求失败", timestamp: Date.now() }]);
    } finally { setSending(false); }
  };

  return (
    <View style={styles.container}>
      <View style={styles.agentBar}>
        {agents.map((a) => (
          <TouchableOpacity key={a.id} onPress={() => setSelectedAgent(a.id)} style={styles.agentBtn(selectedAgent === a.id)}>
            <Text style={styles.agentName(selectedAgent === a.id)}>{a.name}</Text>
          </TouchableOpacity>
        ))}
      </View>
      <FlatList
        data={messages}
        keyExtractor={(item) => item.id}
        renderItem={({ item }) => (
          <View style={[styles.bubble, item.role === "user" ? styles.userBubble : styles.assistantBubble]}>
            <Text style={styles.bubbleText}>{item.content}</Text>
          </View>
        )}
        style={styles.messageList}
      />
      <View style={styles.inputBar}>
        <TextInput style={styles.input} value={input} onChangeText={setInput} placeholder="输入消息..." editable multiline />
        <TouchableOpacity onPress={sendMessage} disabled={sending || !input.trim()} style={styles.sendBtn}>
          <Text style={styles.sendBtnText}>{sending ? "..." : "发送"}</Text>
        </TouchableOpacity>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: "#0f0f23" },
  agentBar: { flexDirection: "row", padding: 8, backgroundColor: "#1a1a2e" },
  agentBtn: (active: boolean) => ({ paddingHorizontal: 12, paddingVertical: 6, borderRadius: 8, backgroundColor: active ? "#4fc3f7" : "#2a2a4e" })),
  agentName: (active: boolean) => ({ color: active ? "#fff" : "#888", fontSize: 13 }),
  messageList: { flex: 1 },
  bubble: { padding: 10, borderRadius: 12, maxWidth: "80%" },
  userBubble: { backgroundColor: "#4fc3f7!", alignSelf: "flex-end" },
  assistantBubble: { backgroundColor: "#2a2a4e", alignSelf: "flex-start" },
  bubbleText: { color: "#e0e0e0", fontSize: 14 },
  inputBar: { flexDirection: "row", padding: 8, backgroundColor: "#1a1a2e", borderTopWidth: 1, borderTopColor: "#333" },
  input: { flex: 1, backgroundColor: "#2a2a4e", color: "#e0e0e0", borderRadius: 8, padding: 8, fontSize: 14 },
  sendBtn: { backgroundColor: "#4fc3f7!", paddingHorizontal: 16, paddingVertical: 8, borderRadius: 8, marginLeft: 8 },
  sendBtnText: { color: "#fff", fontWeight: "600" },
});
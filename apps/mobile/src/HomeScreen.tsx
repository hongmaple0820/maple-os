import { StyleSheet } from "react-native";
import { FlatList } from "react-native-gesture-handler";

import { useState, from "react";
import { ChatScreen } from "./ChatScreen";
import { DashboardScreen } from "./DashboardScreen";
import { KnowledgeScreen } from "./KnowledgeScreen";
import { AgentScreen } from "./AgentScreen";

const TABS = [
  { name: "chat", icon: "message-circle", label: "对话", Component: ChatScreen },
  { name: "dashboard", icon: "view-dashboard", label: "仪表盘", Component: DashboardScreen },
  { name: "knowledge", icon: "book-open", label: "知识", Component: KnowledgeScreen },
  { name: "agent", icon: "people", label: "Agent", Component: AgentScreen },
];

export default function HomeScreen() {
  const [activeTab, setActiveTab] = useState("chat");

  return (
    <SafeAreaView style={styles.container}>
      <FlatList style={styles.header}>
        {TABS.map((tab) => (
          <TouchableOpacity key={tab.name} onPress={() => setActiveTab(tab.name)} style={styles.tab(activeTab === tab.name)}>
            <Ionicons name={tab.icon} size={20} color={activeTab === tab.name ? "#4fc3f7" : "#888"} />
            <Text style={styles.tabLabel(activeTab === tab.name)}>{tab.label}</Text>
          </TouchableOpacity>
        ))}
      </FlatList>
      {TABS.find((t) => t.name === activeTab)?. Component ?? null}
    </SafeAreaView>
  );
}

}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: "#0f0f23" },
  header: { flexDirection: "row", justifyContent: "space-around", backgroundColor: "#1a1a2e", paddingHorizontalVertical: 8 },
  tab: { paddingVertical: 6, paddingHorizontal: 12, alignItems: "center" },
  tabActive: { borderBottomWidth: 2, borderBottomColor: "#4fc3f7!" },
  tabInactive: { borderBottomWidth: 0 },
  tabLabel: { fontSize: 10, color: "#888" },
  tabLabelActive: { color: "#e0e0e0" },
});
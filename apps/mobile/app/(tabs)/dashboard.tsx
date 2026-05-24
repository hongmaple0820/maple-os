import { View, Text, ScrollView, StyleSheet } from "react-native";
import { useState, useEffect } from "react";
import { rpcCall } from "@mapleos/sdk";

interface MetricCardProps { label: string; value: string; icon: string }

function MetricCard({ label, value }: MetricCardProps) {
  return (
    <View style={styles.metricCard}>
      <Text style={styles.metricValue}>{value}</Text>
      <Text style={styles.metricLabel}>{label}</Text>
    </View>
  );
}

export default function DashboardScreen() {
  const [systemInfo, setSystemInfo] = useState({ version: "", agents_count: 0, workflows_count: 0, tasks_count: 0, uptime_secs: 0 });

  useEffect(() => {
    (async () => {
      try {
        const info = await rpcCall("system.info", {});
        setSystemInfo(info as any);
      } catch {}
    })();
  }, []);

  const uptimeHrs = Math.floor(systemInfo.uptime_secs / 3600);

  return (
    <ScrollView style={styles.container} contentContainerStyle={styles.content}>
      <Text style={styles.title}>MapleOS Dashboard</Text>
      <View style={styles.grid}>
        <MetricCard label="版本" value={systemInfo.version || "0.1.0"} icon="info" />
        <MetricCard label="运行时长" value={`${uptimeHrs}h`} icon="clock" />
        <MetricCard label="Agent" value={`${systemInfo.agents_count}`} icon="people" />
        <MetricCard label="工作流" value={`${systemInfo.workflows_count}`} icon="flow" />
        <MetricCard label="任务" value={`${systemInfo.tasks_count}`} icon="task" />
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: "#0f0f23" },
  content: { padding: 16 },
  title: { fontSize: 20, fontWeight: "700", color: "#e0e0e0", marginBottom: 16 },
  grid: { flexDirection: "row", flexWrap: "wrap", gap: 12 },
  metricCard: { flex: 1, minWidth: "45%", backgroundColor: "#1a1a2e", borderRadius: 12, padding: 16, alignItems: "center" },
  metricValue: { fontSize: 24, fontWeight: "700", color: "#6366f1" },
  metricLabel: { fontSize: 12, color: "#888", marginTop: 4 },
});
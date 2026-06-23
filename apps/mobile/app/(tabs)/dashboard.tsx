import { View, Text, ScrollView, StyleSheet } from "react-native";
import { useState, useEffect } from "react";
import { mobileRpcCall } from "../../src/lib/api";

interface MetricCardProps { label: string; value: string }

function MetricCard({ label, value }: MetricCardProps) {
  return (
    <View style={styles.metricCard}>
      <Text style={styles.metricValue}>{value}</Text>
      <Text style={styles.metricLabel}>{label}</Text>
    </View>
  );
}

interface SystemInfoResult {
  version: string;
  uptime_secs: number;
  agents_count: number;
  workflows_count: number;
  tasks_count: number;
}

export default function DashboardScreen() {
  const [systemInfo, setSystemInfo] = useState<SystemInfoResult>({
    version: "-", uptime_secs: 0, agents_count: 0, workflows_count: 0, tasks_count: 0,
  });
  const [error, setError] = useState("");

  useEffect(() => {
    mobileRpcCall<SystemInfoResult>("system.info", {})
      .then(setSystemInfo)
      .catch((err) => setError(err.message));
  }, []);

  const uptimeHrs = Math.floor(systemInfo.uptime_secs / 3600);

  return (
    <ScrollView style={styles.container} contentContainerStyle={styles.content}>
      <Text style={styles.title}>MapleOS Dashboard</Text>
      {error ? <Text style={styles.errorText}>{error}</Text> : null}
      <View style={styles.grid}>
        <MetricCard label="版本" value={systemInfo.version} />
        <MetricCard label="运行时长" value={`${uptimeHrs}h`} />
        <MetricCard label="Agent" value={`${systemInfo.agents_count}`} />
        <MetricCard label="工作流" value={`${systemInfo.workflows_count}`} />
        <MetricCard label="任务" value={`${systemInfo.tasks_count}`} />
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
  errorText: { color: "#f87171", fontSize: 12, marginBottom: 8 },
});
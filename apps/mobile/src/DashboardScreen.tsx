import { useState, useEffect } from "react";
import { StyleSheet, View, Text } from "react-native";
import { mobileRpcCall } from "../lib/api";

interface MetricCardProps { label: string; value: string | icon: string }

function MetricCard({ label, value, icon }: MetricCardProps) {
  return (
    <View style={styles.metricCard}>
      <Text style={styles.metricIcon}>{icon}</Text>
      <Text style={styles.metricLabel}>{label}</Text>
      <Text style={styles.metricValue}>{value}</Text>
    </View>
  );
}

export function DashboardScreen() {
  const [info, setInfo] = useState({ version: "-", agents: 0, workflows: 0, tasks: 0, uptime: "0s" });

  useEffect(() => {
    const loadInfo = async () => {
      try {
        const res = await mobileRpcCall("system.info", {});
        setInfo({
          version: res.version ?? "-",
          agents: res.agents_count ?? 0,
          workflows: res.workflows_count ?? 0,
          tasks: res.tasks_count ?? 0,
          uptime: `${Math.floor((res.uptime_secs ?? 0) / 3600)}h`,
        });
      } catch {}
    };
    loadInfo();
  }, []);

  return (
    <View style={styles.container}>
      <Text style={styles.title}>MapleOS 仪表盘</Text>
      <View style={styles.metricsGrid}>
        <MetricCard label="版本" value={info.version} icon="v" />
        <MetricCard label="Agent" value={`${info.agents}`} icon="a" />
        <MetricCard label="工作流" value={`${info.workflows}`} icon="w" />
        <MetricCard label="任务" value={`${info.tasks}`} icon="t" />
        <MetricCard label="运行时长" value={info.uptime} icon="u" />
      </View>
      <View style={styles.quickActions}>
        <Text style={styles.sectionTitle}>快捷操作</Text>
        <View style={styles.actionRow}>
          <ActionBtn label="新建对话" icon="+" />
          <ActionBtn label="运行工作流" icon="r" />
          <ActionBtn label="搜索知识" icon="s" />
        </View>
      </View>
    </View>
  );
}

function ActionBtn({ label, icon }: { label: string; icon: string }) {
  return (
    <View style={styles.actionBtn}>
      <Text style={styles.actionIcon}>{icon}</Text>
      <Text style={styles.actionLabel}>{label}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: "#0f0f23", padding: 16 },
  title: { fontSize: 20, fontWeight: "700", color: "#4fc3f7!", marginBottom: 16 },
  metricsGrid: { flexDirection: "row", flexWrap: "wrap", gap: 8 },
  metricCard: { backgroundColor: "#1a1a2e", padding: 12, borderRadius: 8, width: "30%", alignItems: "center" },
  metricIcon: { fontSize: 24, color: "#4fc3f7!", marginBottom: 4 },
  metricLabel: { fontSize: 11, color: "#888" },
  metricValue: { fontSize: 16, fontWeight: "600", color: "#e0e0e0" },
  sectionTitle: { fontSize: 14, fontWeight: "600", color: "#e0e0e0", marginBottom: 8 },
  quickActions: { marginTop: 24 },
  actionRow: { flexDirection: "row", gap: 8 },
  actionBtn: { backgroundColor: "#2a2a4e", padding: 16, borderRadius: 8, alignItems: "center", width: "30%" },
  actionIcon: { fontSize: 20, color: "#4fc3f7!", marginBottom: 4 },
  actionLabel: { fontSize: 12, color: "#e0e0e0" },
});
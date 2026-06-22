// Workflow types and constants extracted from workflow-manager.tsx (#93)
// This file contains shared types, color mappings, and palette items.

import type { Node, Edge } from "@xyflow/react";

export interface WFNodeData {
  label: string;
  labelKey?: string;
  nodeType: "llm" | "tool" | "condition" | "human_approval" | "trigger";
  model?: string;
  skillId?: string;
  expression?: string;
  status?: "idle" | "running" | "completed" | "failed" | "waiting";
  [key: string]: unknown;
}

export type RFNode = Node<WFNodeData>;
export type RFEdge = Edge;

export const nodeTypeLabel: Record<string, string> = {
  llm: "workflow.nodeTypes.llm",
  tool: "workflow.nodeTypes.tool",
  condition: "workflow.nodeTypes.condition",
  human_approval: "workflow.nodeTypes.humanApproval",
  trigger: "workflow.nodeTypes.trigger",
};

export const nodeTypeColor: Record<string, { bg: string; border: string; accent: string }> = {
  llm: { bg: "bg-blue-50", border: "border-blue-200", accent: "text-blue-500" },
  tool: { bg: "bg-yellow-50", border: "border-yellow-200", accent: "text-yellow-500" },
  condition: { bg: "bg-purple-50", border: "border-purple-200", accent: "text-purple-500" },
  human_approval: { bg: "bg-orange-50", border: "border-orange-200", accent: "text-orange-500" },
  trigger: { bg: "bg-green-50", border: "border-green-200", accent: "text-green-500" },
};

export const statusBorder: Record<string, string> = {
  idle: "border",
  running: "border-2 border-warning animate-pulse",
  completed: "border-2 border-success",
  failed: "border-2 border-destructive",
  waiting: "border-2 border-muted-foreground",
};

export const PALETTE_ITEMS = [
  { type: "llm" as const, labelKey: "workflow.nodeTypes.llm" },
  { type: "tool" as const, labelKey: "workflow.nodeTypes.tool" },
  { type: "condition" as const, labelKey: "workflow.nodeTypes.condition" },
  { type: "human_approval" as const, labelKey: "workflow.nodeTypes.humanApproval" },
  { type: "trigger" as const, labelKey: "workflow.nodeTypes.trigger" },
];

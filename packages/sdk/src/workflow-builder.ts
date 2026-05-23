export interface WorkflowTriggerCron {
  type: 'cron';
  expression: string;
  timezone: string;
}

export interface WorkflowTriggerWebhook {
  type: 'webhook';
  path: string;
  method: string;
}

export type WorkflowTrigger = WorkflowTriggerCron | WorkflowTriggerWebhook;

export interface WorkflowNodeLlm {
  id: string;
  name: string;
  type: 'llm';
  model_route: string;
  prompt_ref: string;
  temperature: number;
  depends_on: string[];
}

export interface WorkflowNodeTool {
  id: string;
  name: string;
  type: 'tool';
  skill_id: string;
  config: Record<string, unknown>;
  depends_on: string[];
}

export interface WorkflowNodeConditionBranch {
  label: string;
  target_node: string;
}

export interface WorkflowNodeCondition {
  id: string;
  name: string;
  type: 'condition';
  expression: string;
  branches: WorkflowNodeConditionBranch[];
  depends_on: string[];
}

export interface WorkflowNodeHumanApproval {
  id: string;
  name: string;
  type: 'human_approval';
  timeout_secs: number;
  on_timeout: 'auto_approve' | 'auto_reject' | 'fail_workflow';
  depends_on: string[];
}

export type WorkflowNode = WorkflowNodeLlm | WorkflowNodeTool | WorkflowNodeCondition | WorkflowNodeHumanApproval;

export interface WorkflowDefinition {
  name: string;
  description: string;
  version: string;
  trigger: WorkflowTrigger;
  variables: Record<string, unknown>;
  nodes: WorkflowNode[];
  hooks: Record<string, unknown>;
}

export class WorkflowBuilder {
  private workflow: WorkflowDefinition = {
    name: '',
    description: '',
    version: '1.0',
    trigger: { type: 'cron', expression: '', timezone: 'UTC' },
    variables: {},
    nodes: [],
    hooks: {},
  };

  setName(name: string): this {
    this.workflow.name = name;
    return this;
  }

  setDescription(desc: string): this {
    this.workflow.description = desc;
    return this;
  }

  setCronTrigger(expression: string, timezone = 'UTC'): this {
    this.workflow.trigger = { type: 'cron', expression, timezone };
    return this;
  }

  setWebhookTrigger(path: string, method = 'POST'): this {
    this.workflow.trigger = { type: 'webhook', path, method };
    return this;
  }

  addVariable(key: string, value: unknown): this {
    this.workflow.variables[key] = value;
    return this;
  }

  addLlmNode(id: string, config: {
    modelRoute: string;
    promptRef: string;
    dependsOn?: string[];
    temperature?: number;
  }): this {
    const node: WorkflowNodeLlm = {
      id,
      name: id,
      type: 'llm',
      model_route: config.modelRoute,
      prompt_ref: config.promptRef,
      temperature: config.temperature ?? 0.7,
      depends_on: config.dependsOn ?? [],
    };
    this.workflow.nodes.push(node);
    return this;
  }

  addToolNode(id: string, config: {
    skillId: string;
    config?: Record<string, unknown>;
    dependsOn?: string[];
  }): this {
    const node: WorkflowNodeTool = {
      id,
      name: id,
      type: 'tool',
      skill_id: config.skillId,
      config: config.config ?? {},
      depends_on: config.dependsOn ?? [],
    };
    this.workflow.nodes.push(node);
    return this;
  }

  addConditionNode(id: string, config: {
    expression: string;
    branches: Array<{ label: string; targetNode: string }>;
    dependsOn?: string[];
  }): this {
    const node: WorkflowNodeCondition = {
      id,
      name: id,
      type: 'condition',
      expression: config.expression,
      branches: config.branches.map((b) => ({
        label: b.label,
        target_node: b.targetNode,
      })),
      depends_on: config.dependsOn ?? [],
    };
    this.workflow.nodes.push(node);
    return this;
  }

  addHumanApprovalNode(id: string, config: {
    timeoutSecs: number;
    onTimeout: 'auto_approve' | 'auto_reject' | 'fail_workflow';
    dependsOn?: string[];
  }): this {
    const node: WorkflowNodeHumanApproval = {
      id,
      name: id,
      type: 'human_approval',
      timeout_secs: config.timeoutSecs,
      on_timeout: config.onTimeout,
      depends_on: config.dependsOn ?? [],
    };
    this.workflow.nodes.push(node);
    return this;
  }

  build(): WorkflowDefinition {
    return { ...this.workflow, nodes: [...this.workflow.nodes] };
  }

  toYaml(): string {
    return JSON.stringify(this.workflow, null, 2);
  }
}
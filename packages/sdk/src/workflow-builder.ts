export class WorkflowBuilder {
  private workflow: any = {
    name: '',
    description: '',
    version: '1.0',
    trigger: {},
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

  addLlmNode(id: string, config: {
    modelRoute: string;
    promptRef: string;
    dependsOn?: string[];
    temperature?: number;
  }): this {
    this.workflow.nodes.push({
      id,
      name: id,
      type: 'llm',
      model_route: config.modelRoute,
      prompt_ref: config.promptRef,
      temperature: config.temperature ?? 0.7,
      depends_on: config.dependsOn ?? [],
    });
    return this;
  }

  addToolNode(id: string, config: {
    skillId: string;
    config?: any;
    dependsOn?: string[];
  }): this {
    this.workflow.nodes.push({
      id,
      name: id,
      type: 'tool',
      skill_id: config.skillId,
      config: config.config ?? {},
      depends_on: config.dependsOn ?? [],
    });
    return this;
  }

  addConditionNode(id: string, config: {
    expression: string;
    branches: Array<{ label: string; targetNode: string }>;
    dependsOn?: string[];
  }): this {
    this.workflow.nodes.push({
      id,
      name: id,
      type: 'condition',
      expression: config.expression,
      branches: config.branches.map((b) => ({
        label: b.label,
        target_node: b.targetNode,
      })),
      depends_on: config.dependsOn ?? [],
    });
    return this;
  }

  addHumanApprovalNode(id: string, config: {
    timeoutSecs: number;
    onTimeout: 'auto_approve' | 'auto_reject' | 'fail_workflow';
    dependsOn?: string[];
  }): this {
    this.workflow.nodes.push({
      id,
      name: id,
      type: 'human_approval',
      timeout_secs: config.timeoutSecs,
      on_timeout: config.onTimeout,
      depends_on: config.dependsOn ?? [],
    });
    return this;
  }

  build(): any {
    return { ...this.workflow };
  }

  toYaml(): string {
    return JSON.stringify(this.workflow, null, 2);
  }
}

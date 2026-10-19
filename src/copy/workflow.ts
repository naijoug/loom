export const WORKFLOW_COPY = {
  locale: "zh-CN",
  stages: {
    planning: "规划",
    implementation: "实施",
    testing: "测试验收",
    done: "完成",
  },
  actions: {
    confirmPlan: "确认计划并生成任务",
    markReadyForTesting: "进入测试验收",
    acceptTask: "确认验收并完成",
    createFollowUp: "开始后续任务",
  },
  blockers: {
    prefix: "暂时不能继续：",
    acceptancePrefix: "暂时不能验收：",
  },
  terms: {
    agent: "Agent",
    review: "Review",
    testing: "测试验收",
    evidence: "证据",
  },
} as const;

const RUN_STATUS_COPY: Record<string, string> = {
  running: "运行中",
  succeeded: "已通过",
  failed: "失败",
  cancelled: "已取消",
  interrupted: "已中断",
  pending: "等待中",
  blocked: "已阻塞",
};

const TASK_STATUS_COPY: Record<string, string> = {
  drafting_requirements: "需求起草",
  planning: "规划中",
  plan_review: "计划 Review",
  ready_to_implement: "待实施",
  implementing: "实施中",
  reviewing: "实施 Review",
  debugging: "测试验收",
  fixing: "修复中",
  verifying: "验证中",
  completed: "已完成",
  blocked: "已阻塞",
  cancelled: "已取消",
};

export function runStatusCopy(status: string) {
  return RUN_STATUS_COPY[status] ?? status.split("_").join(" ");
}

export function formatRunStatus(status: string, exitCode?: number) {
  const label = runStatusCopy(status);
  return typeof exitCode === "number" ? `${label} · 退出码 ${exitCode}` : label;
}

export function taskStatusCopy(status: string) {
  return TASK_STATUS_COPY[status] ?? status.split("_").join(" ");
}

import { openPath } from "@tauri-apps/plugin-opener";
import { save } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react";
import {
  ArrowLeft,
  Bot,
  CheckCircle2,
  ExternalLink,
  Info,
  Palette,
  Pencil,
  Plus,
  RefreshCw,
  ShieldCheck,
  SlidersHorizontal,
  Trash2,
  XCircle,
} from "lucide-react";
import { invokeCommand, TAURI_COMMANDS } from "../../api";
import { useTheme } from "../../contexts/ThemeContext";
import {
  DEFAULT_APP_SETTINGS,
  EMPTY_PROJECT_AGENT_PREFERENCES,
  type AgentAdapterType,
  type AgentCapability,
  type AgentConfig,
  type AgentConfigInput,
  type AgentDiagnostic,
  type AppSettings,
  type HealthCheckResult,
  type ProjectAgentPreferences,
  type TerminalSlot,
} from "../../domain";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { hasTauriRuntime } from "../../hooks/runtime";
import { useSettingsBridge } from "../../hooks/useSettingsBridge";
import { useProjectPreferencesBridge } from "../../hooks/useProjectPreferencesBridge";
import { useTerminalBridge } from "../../hooks/useTerminalBridge";
import { useAppState } from "../../state/AppStateContext";
import {
  applyTerminalSlotDraft,
  blankTerminalSlotDraft,
  draftFromTerminalSlot,
  resolveTerminalSlotCwd,
  type TerminalSlotDraft,
} from "../../utils/terminalSlots";
import { Button } from "../../components/common/Button";
import { defaultAgentDraft, draftFromAgent, isBuiltInAgent, profileSummary } from "./agentDraft";
import { Field, SettingCard, Switch, ThemePreview, ToggleControl } from "./controls";
import "./SettingsPage.css";

interface SettingsPageProps {
  onBack: () => void;
  initialTab?: SettingsTab;
}

type SettingsTab = "general" | "appearance" | "agents" | "safety" | "about";

const cliAdapterTypes = new Set(["codex_cli", "claude_code_cli", "dummy", "cli"]);
const capabilityOptions: AgentCapability[] = [
  "planning",
  "implementation",
  "review",
  "debugging",
  "testing",
  "documentation",
];
const capabilityLabels: Record<AgentCapability, string> = {
  planning: "规划",
  implementation: "实施",
  review: "Review",
  debugging: "调试",
  testing: "测试",
  documentation: "文档",
};
const adapterOptions: Array<{ value: AgentAdapterType; label: string; command: string; name: string }> = [
  { value: "codex_cli", label: "Codex CLI", command: "codex", name: "Codex" },
  { value: "claude_code_cli", label: "Claude Code", command: "claude", name: "Claude Code" },
  { value: "cli", label: "Custom CLI", command: "", name: "Custom Agent" },
  { value: "dummy", label: "Dummy/Test", command: "dummy", name: "Dummy Agent" },
];
const settingsTabs: Array<{ id: SettingsTab; label: string; icon: ReactNode }> = [
  { id: "general", label: "通用", icon: <SlidersHorizontal size={15} /> },
  { id: "appearance", label: "外观", icon: <Palette size={15} /> },
  { id: "agents", label: "Agent", icon: <Bot size={15} /> },
  { id: "safety", label: "命令与安全", icon: <ShieldCheck size={15} /> },
  { id: "about", label: "关于", icon: <Info size={15} /> },
];

function shortTimestamp(timestampMs?: number) {
  if (!timestampMs) {
    return "暂无";
  }
  return new Date(timestampMs).toLocaleString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    year: "numeric",
    month: "short",
    day: "2-digit",
  });
}

export function SettingsPage({ onBack, initialTab = "general" }: SettingsPageProps) {
  const { theme, themeMode, setThemeMode } = useTheme();
  const { state } = useAppState();
  const { createAgent, deleteAgent, diagnoseAgents, loadAgents, setAgentEnabled, updateAgent } = useAgentBridge();
  const { loadSettings, saveSettings } = useSettingsBridge();
  const { loadProjectAgentPreferences, saveProjectAgentPreferences } = useProjectPreferencesBridge();
  const { listTerminalSlots, saveTerminalSlots, suggestTerminalSlots } = useTerminalBridge();
  const [activeTab, setActiveTab] = useState<SettingsTab>(initialTab);
  const [agentDraft, setAgentDraft] = useState<AgentConfigInput>(defaultAgentDraft);
  const [argsText, setArgsText] = useState("");
  const [editingAgentId, setEditingAgentId] = useState<string | null>(null);
  const [appSettings, setAppSettings] = useState<AppSettings>(DEFAULT_APP_SETTINGS);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [projectPreferences, setProjectPreferences] = useState<ProjectAgentPreferences>({
    ...EMPTY_PROJECT_AGENT_PREFERENCES,
  });
  const [projectPreferencesError, setProjectPreferencesError] = useState<string | null>(null);
  const [agentDiagnostics, setAgentDiagnostics] = useState<AgentDiagnostic[]>([]);
  const [health, setHealth] = useState<HealthCheckResult | null>(null);
  const [slots, setSlots] = useState<TerminalSlot[]>([]);
  const [slotsLoaded, setSlotsLoaded] = useState(false);
  const [slotDraft, setSlotDraft] = useState<TerminalSlotDraft | null>(null);
  const [slotError, setSlotError] = useState<string | null>(null);
  const [includeDiagnosticLogs, setIncludeDiagnosticLogs] = useState(false);
  const [diagnosticNotice, setDiagnosticNotice] = useState<string | null>(null);

  const project = state.projects.current;

  useEffect(() => {
    void loadAgents();
    void diagnoseAgents().then(setAgentDiagnostics);
  }, [diagnoseAgents, loadAgents]);

  useEffect(() => {
    let cancelled = false;
    void loadSettings()
      .then((loaded) => {
        if (!cancelled) {
          setAppSettings(loaded);
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setSettingsError(error instanceof Error ? error.message : String(error));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [loadSettings]);

  useEffect(() => {
    let cancelled = false;
    setProjectPreferencesError(null);
    if (!project) {
      setProjectPreferences({ ...EMPTY_PROJECT_AGENT_PREFERENCES });
      return;
    }
    void loadProjectAgentPreferences(project.path)
      .then((loaded) => {
        if (!cancelled) {
          setProjectPreferences(loaded);
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setProjectPreferencesError(error instanceof Error ? error.message : String(error));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [project, loadProjectAgentPreferences]);

  useEffect(() => {
    if (!hasTauriRuntime()) {
      setHealth({
        status: "ok",
        app: "Loom",
        version: "preview",
        backend: "tauri",
        timestampMs: Date.now(),
      });
      return;
    }

    let cancelled = false;
    void invokeCommand<HealthCheckResult>(TAURI_COMMANDS.healthCheck).then((result) => {
      if (!cancelled) {
        setHealth(result);
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    setSlotsLoaded(false);
    setSlotDraft(null);
    setSlotError(null);

    if (!project) {
      setSlots([]);
      setSlotsLoaded(true);
      return;
    }

    void listTerminalSlots(project.path).then(async (loaded) => {
      const next = loaded.length > 0 ? loaded : await suggestTerminalSlots(project.path);
      if (!cancelled) {
        setSlots(next);
        setSlotsLoaded(true);
      }
    });

    return () => {
      cancelled = true;
    };
  }, [project, listTerminalSlots, suggestTerminalSlots]);

  const cliAgents = state.agents.filter((agent) => cliAdapterTypes.has(agent.adapterType));
  const selectedAdapter = adapterOptions.find((option) => option.value === agentDraft.adapterType);
  const editingAgent = editingAgentId
    ? cliAgents.find((agent) => agent.id === editingAgentId) ?? null
    : null;
  const availableAgents = cliAgents.filter((agent) => agent.available);
  const phaseCoverage = useMemo(() => new Set(cliAgents.flatMap((agent) => agent.capabilities)), [cliAgents]);

  function updateAppSettings(patch: Partial<AppSettings>) {
    const next = { ...appSettings, ...patch };
    setAppSettings(next);
    setSettingsError(null);
    void saveSettings(next)
      .then((saved) => setAppSettings(saved))
      .catch((error: unknown) => {
        setSettingsError(error instanceof Error ? error.message : String(error));
      });
  }

  function updateProjectPreferences(patch: Partial<ProjectAgentPreferences>) {
    if (!project) {
      return;
    }
    const next = { ...projectPreferences, ...patch };
    setProjectPreferences(next);
    setProjectPreferencesError(null);
    void saveProjectAgentPreferences(project.path, next)
      .then((saved) => setProjectPreferences(saved))
      .catch((error: unknown) => {
        setProjectPreferencesError(error instanceof Error ? error.message : String(error));
      });
  }

  function toggleProjectPreference(
    key: "planningAgentIds" | "reviewAgentIds",
    agentId: string,
  ) {
    const current = projectPreferences[key];
    updateProjectPreferences({
      [key]: current.includes(agentId)
        ? current.filter((id) => id !== agentId)
        : [...current, agentId],
    });
  }

  function handleAdapterChange(adapterType: AgentAdapterType) {
    const adapter = adapterOptions.find((option) => option.value === adapterType);

    setAgentDraft((current) => ({
      ...current,
      adapterType,
      name: current.name.trim() && current.name !== selectedAdapter?.name ? current.name : adapter?.name ?? current.name,
      command: adapterType === "cli" ? current.command : adapter?.command ?? current.command,
      canWriteFiles: adapterType === "dummy" ? false : current.canWriteFiles,
      canRunCommands: adapterType === "dummy" ? false : current.canRunCommands,
    }));
  }

  function toggleCapability(capability: AgentCapability) {
    setAgentDraft((current) => {
      const hasCapability = current.capabilities.includes(capability);
      const capabilities = hasCapability
        ? current.capabilities.filter((value) => value !== capability)
        : [...current.capabilities, capability];

      return { ...current, capabilities };
    });
  }

  async function handleCreateAgent(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!agentDraft.name.trim() || !agentDraft.command.trim() || agentDraft.capabilities.length === 0) {
      return;
    }

    const input = {
      ...agentDraft,
      name: agentDraft.name.trim(),
      command: agentDraft.command.trim(),
      args: argsText
        .split(/\r?\n/)
        .map((arg) => arg.trim())
        .filter(Boolean),
    };
    const result = editingAgentId
      ? await updateAgent(editingAgentId, input)
      : await createAgent(input);

    if (result) {
      setAgentDraft(defaultAgentDraft());
      setArgsText("");
      setEditingAgentId(null);
    }
  }

  function handleEditAgent(agent: AgentConfig) {
    if (isBuiltInAgent(agent)) {
      return;
    }
    setEditingAgentId(agent.id);
    setAgentDraft(draftFromAgent(agent));
    setArgsText(agent.args.join("\n"));
    setActiveTab("agents");
  }

  function handleCancelEdit() {
    setEditingAgentId(null);
    setAgentDraft(defaultAgentDraft());
    setArgsText("");
  }

  async function persistSlots(next: TerminalSlot[]) {
    if (!project) {
      return;
    }
    setSlotError(null);
    const saved = await saveTerminalSlots(project.path, next);
    if (saved) {
      setSlots(saved);
    } else {
      setSlotError("当前环境无法保存终端槽位。");
    }
  }

  function saveSlotDraft() {
    if (!slotDraft) {
      return;
    }
    const next = applyTerminalSlotDraft(slots, slotDraft);
    if (!next) {
      return;
    }
    void persistSlots(next);
    setSlotDraft(null);
  }

  function removeSlot(slot: TerminalSlot) {
    void persistSlots(slots.filter((candidate) => candidate.id !== slot.id));
  }

  async function resetSlotsToDetected() {
    if (!project) {
      return;
    }
    const detected = await suggestTerminalSlots(project.path);
    await persistSlots(detected);
  }

  function openResource(path: string) {
    if (!hasTauriRuntime()) {
      return;
    }
    void openPath(path);
  }

  async function exportProjectDiagnostics() {
    if (!project || !hasTauriRuntime()) return;
    const targetPath = await save({
      defaultPath: `${project.name.replace(/[^\p{L}\p{N}._-]+/gu, "-") || "loom-project"}-diagnostics.json`,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (!targetPath) return;
    setDiagnosticNotice(null);
    try {
      const exported = await invokeCommand<string>(TAURI_COMMANDS.exportDiagnosticBundle, {
        input: {
          projectPath: project.path,
          taskId: state.app.selectedTaskId ?? undefined,
          targetPath,
          includeLogTails: includeDiagnosticLogs,
        },
      });
      setDiagnosticNotice(`脱敏诊断包已导出到 ${exported}`);
    } catch (error) {
      setDiagnosticNotice(error instanceof Error ? error.message : String(error));
    }
  }

  function renderGeneral() {
    return (
      <>
        <SettingCard title="当前项目">
          <Field
            title="项目"
            description={project ? project.path : "请先打开本地项目，再配置项目专属设置。"}
            control={<span className="settings-input-pill">{project?.name ?? "未打开项目"}</span>}
          />
          <Field
            title="代码仓库"
            description={project?.gitBranch ? `当前分支：${project.gitBranch}` : "当前项目是 Git 仓库时会显示分支。"}
            control={<span className={`settings-tag ${project?.isGitRepository ? "ok" : ""}`}>{project?.isGitRepository ? "Git" : "非 Git"}</span>}
          />
          <Field
            title="Loom 项目存储"
            description=".loom 保存本地任务历史、日志、终端槽位和验证证据。"
            control={<span className={`settings-tag ${project?.loomDirReady ? "ok" : ""}`}>{project?.loomDirReady ? "就绪" : "未初始化"}</span>}
          />
        </SettingCard>
        <SettingCard title="运行模式">
          <Field
            title="数据位置"
            description="Agent 配置和应用设置只保存在桌面应用的本地数据目录。"
            control={<span className="settings-chip">仅本地</span>}
          />
          <Field
            title="项目技术栈"
            description="检测到的技术栈用于生成命令建议和默认终端槽位。"
            control={
              <span className="settings-row-control wrap">
                {(project?.detectedStacks.length ? project.detectedStacks : ["未检测到"]).map((stack) => (
                  <span className="settings-chip" key={stack}>{stack}</span>
                ))}
              </span>
            }
          />
        </SettingCard>
      </>
    );
  }

  function renderAppearance() {
    return (
      <>
        <SettingCard title="主题">
          <div className="theme-grid">
            <ThemePreview label="浅色" mode="light" active={themeMode === "light"} onClick={() => setThemeMode("light")} />
            <ThemePreview label="深色" mode="dark" active={themeMode === "dark"} onClick={() => setThemeMode("dark")} />
            <ThemePreview label="跟随系统" mode="system" active={themeMode === "system"} onClick={() => setThemeMode("system")} />
          </div>
        </SettingCard>
        <SettingCard title="界面">
          <Field
            title="当前主题"
            description="跟随系统模式会使用操作系统主题，直到你明确选择浅色或深色。"
            control={<span className="settings-chip">{theme === "light" ? "浅色" : "深色"}</span>}
          />
          <Field
            title="字体与密度"
            description="Loom 当前使用内置界面字体和固定工作台密度。"
            control={<span className="settings-chip">内置</span>}
          />
        </SettingCard>
      </>
    );
  }

  function renderAgents() {
    const stageAgents = (capabilities: AgentCapability[]) => cliAgents.filter(
      (agent) => agent.enabled && agent.capabilities.some((capability) => capabilities.includes(capability)),
    );
    const agentSelect = (
      value: string | undefined,
      capabilities: AgentCapability[],
      onChange: (agentId: string | undefined) => void,
    ) => (
      <select
        className="settings-stage-select"
        value={value ?? ""}
        disabled={!project}
        onChange={(event) => onChange(event.target.value || undefined)}
      >
        <option value="">不设默认值</option>
        {stageAgents(capabilities).map((agent) => (
          <option key={agent.id} value={agent.id}>{agent.name}{agent.available ? "" : "（不可用）"}</option>
        ))}
      </select>
    );

    return (
      <>
        <div className="settings-page-heading">
          <div>
            <div className="settings-sub">可由 Loom 编排的本地 AI 编程 Agent</div>
          </div>
          <div className="settings-heading-actions">
            <Button type="button" variant="ghost" iconLeft={<RefreshCw size={14} />} onClick={() => {
              void loadAgents();
              void diagnoseAgents().then(setAgentDiagnostics);
            }}>
              刷新诊断
            </Button>
            <Button
              type="button"
              variant="primary"
              iconLeft={<Plus size={14} />}
              onClick={() => {
                handleCancelEdit();
                setActiveTab("agents");
              }}
            >
              添加 Agent
            </Button>
          </div>
        </div>
        <div className="settings-tiles">
          <div className="settings-tile"><span>总数</span><strong>{cliAgents.length}</strong></div>
          <div className="settings-tile"><span>可用</span><strong className="ok">{availableAgents.length}</strong></div>
          <div className="settings-tile"><span>不可达</span><strong className="err">{cliAgents.length - availableAgents.length}</strong></div>
          <div className="settings-tile"><span>阶段覆盖</span><strong>{Math.min(phaseCoverage.size, 4)}/4</strong></div>
        </div>
        <SettingCard title="项目阶段默认值">
          <Field
            title="项目范围"
            description={project ? `保存在 ${project.path}/.loom/agent-preferences.json，并应用于新任务。` : "打开项目后可配置阶段默认值。"}
            control={<span className="settings-chip">{project?.name ?? "无项目"}</span>}
          />
          <Field
            title="规划 Agent"
            description="新建规划讨论时自动选择的 Agent。"
            control={
              <span className="settings-row-control wrap">
                {stageAgents(["planning"]).map((agent) => (
                  <label className="settings-check" key={agent.id}>
                    <input
                      type="checkbox"
                      disabled={!project}
                      checked={projectPreferences.planningAgentIds.includes(agent.id)}
                      onChange={() => toggleProjectPreference("planningAgentIds", agent.id)}
                    />
                    <span>{agent.name}</span>
                  </label>
                ))}
              </span>
            }
          />
          <Field
            title="实施 Agent"
            description="新任务未显式选择时继承的主 Agent。"
            control={agentSelect(projectPreferences.implementationAgentId, ["implementation"], (implementationAgentId) => updateProjectPreferences({ implementationAgentId }))}
          />
          <Field
            title="Review Agent"
            description="用于实施 Review 和计划 Review 的协作者。"
            control={
              <span className="settings-row-control wrap">
                {stageAgents(["review"]).map((agent) => (
                  <label className="settings-check" key={agent.id}>
                    <input
                      type="checkbox"
                      disabled={!project}
                      checked={projectPreferences.reviewAgentIds.includes(agent.id)}
                      onChange={() => toggleProjectPreference("reviewAgentIds", agent.id)}
                    />
                    <span>{agent.name}</span>
                  </label>
                ))}
              </span>
            }
          />
          <Field
            title="调试 Agent"
            description="用于修复和反馈驱动调试的默认 Agent。"
            control={agentSelect(projectPreferences.debuggingAgentId, ["debugging", "implementation"], (debuggingAgentId) => updateProjectPreferences({ debuggingAgentId }))}
          />
          <Field
            title="测试 Agent"
            description="用于验证失败和测试分析的默认 Agent。"
            control={agentSelect(projectPreferences.testingAgentId, ["testing"], (testingAgentId) => updateProjectPreferences({ testingAgentId }))}
          />
          <Field
            title="文档 Agent"
            description="用于任务总结和文档工作的默认 Agent。"
            control={agentSelect(projectPreferences.documentationAgentId, ["documentation"], (documentationAgentId) => updateProjectPreferences({ documentationAgentId }))}
          />
          {projectPreferencesError && <div className="settings-inline-error">{projectPreferencesError}</div>}
        </SettingCard>
        <SettingCard title="已安装 Agent">
          <div className="settings-table-wrap">
            <table className="settings-agent-table">
              <thead>
                <tr>
                  <th>Agent 与可执行文件</th>
                  <th>适配器</th>
                  <th>能力</th>
                  <th>权限</th>
                  <th>状态</th>
                  <th>实施能力</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {cliAgents.map((agent) => {
                  const diagnostic = agentDiagnostics.find((candidate) => candidate.agentId === agent.id);
                  return (
                  <tr key={agent.id}>
                    <td>
                      <strong>{agent.name}</strong>
                      <code>{profileSummary(agent)}</code>
                    </td>
                    <td><span className="settings-chip">{agent.adapterType}</span></td>
                    <td>
                      <div className="settings-tag-row">
                        {agent.capabilities.map((capability) => (
                          <span className="settings-tag" key={capability}>{capabilityLabels[capability]}</span>
                        ))}
                      </div>
                    </td>
                    <td>
                      <span className="settings-perm"><Switch on={agent.canWriteFiles} /> 文件</span>
                      <span className="settings-perm"><Switch on={agent.canRunCommands} /> 命令</span>
                    </td>
                    <td>
                      <span className={`settings-pill ${diagnostic?.status === "ready" ? "ok" : "err"}`} title={diagnostic?.detail}>
                        {diagnostic?.status === "ready" ? <CheckCircle2 size={13} /> : <XCircle size={13} />}
                        {diagnostic?.status === "ready"
                          ? "就绪"
                          : diagnostic?.status === "disabled"
                            ? "已停用"
                            : diagnostic?.status === "missing"
                              ? "缺失"
                              : agent.available ? "可用" : "缺失"}
                      </span>
                      {diagnostic?.version && <code title={diagnostic.resolvedPath}>{diagnostic.version}</code>}
                      {diagnostic?.resolvedPath && <span className="settings-diagnostic-path" title={diagnostic.resolvedPath}>{diagnostic.resolvedPath}</span>}
                    </td>
                    <td>{agent.capabilities.includes("implementation") ? <span className="settings-tag primary">支持</span> : <span className="settings-dim">不支持</span>}</td>
                    <td>
                      <div className="settings-agent-actions">
                        {isBuiltInAgent(agent) ? (
                          <span className="settings-dim">内置</span>
                        ) : (
                          <button type="button" className="settings-icon-control" title={`编辑 ${agent.name}`} onClick={() => handleEditAgent(agent)}>
                            <Pencil size={14} />
                          </button>
                        )}
                        <button
                          type="button"
                          className="settings-text-control"
                          onClick={() => void setAgentEnabled(agent.id, !agent.enabled)}
                        >
                          {agent.enabled ? "停用" : "启用"}
                        </button>
                        {!isBuiltInAgent(agent) && (
                          <button type="button" className="settings-icon-control danger" title={`删除 ${agent.name}`} onClick={() => void deleteAgent(agent.id)}>
                            <Trash2 size={14} />
                          </button>
                        )}
                      </div>
                    </td>
                  </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          <form className="settings-agent-form" onSubmit={handleCreateAgent}>
            <div className="settings-form-header">
              <div>
                <div className="settings-row-title">{editingAgent ? `编辑 ${editingAgent.name}` : "添加自定义 Agent"}</div>
                <div className="settings-row-desc">通过适配器接口保存可复用 CLI 配置；内置配置只能启用或停用。</div>
              </div>
              <div className="settings-form-actions">
                {editingAgent && (
                  <button type="button" className="settings-text-control" onClick={handleCancelEdit}>
                    取消
                  </button>
                )}
                <Button type="submit" variant="primary" disabled={!agentDraft.name.trim() || !agentDraft.command.trim()}>
                  {editingAgent ? "保存 Agent" : "添加 Agent"}
                </Button>
              </div>
            </div>
            <div className="settings-form-grid">
              <label className="settings-form-field">
                <span>名称</span>
                <input value={agentDraft.name} onChange={(event) => setAgentDraft((current) => ({ ...current, name: event.target.value }))} />
              </label>
              <label className="settings-form-field">
                <span>适配器</span>
                <select value={agentDraft.adapterType} onChange={(event) => handleAdapterChange(event.target.value as AgentAdapterType)}>
                  {adapterOptions.map((option) => (
                    <option key={option.value} value={option.value}>{option.label}</option>
                  ))}
                </select>
              </label>
              <label className="settings-form-field">
                <span>命令</span>
                <input value={agentDraft.command} onChange={(event) => setAgentDraft((current) => ({ ...current, command: event.target.value }))} />
              </label>
              <label className="settings-form-field wide">
                <span>参数（每行一个）</span>
                <textarea value={argsText} onChange={(event) => setArgsText(event.target.value)} placeholder="留空则使用适配器默认参数。" />
              </label>
            </div>
            <div className="settings-check-row">
              {capabilityOptions.map((capability) => (
                <label className="settings-check" key={capability}>
                  <input type="checkbox" checked={agentDraft.capabilities.includes(capability)} onChange={() => toggleCapability(capability)} />
                  <span>{capabilityLabels[capability]}</span>
                </label>
              ))}
            </div>
            <div className="settings-check-row">
              <label className="settings-check">
                <input type="checkbox" checked={agentDraft.canWriteFiles} disabled={agentDraft.adapterType === "dummy"} onChange={(event) => setAgentDraft((current) => ({ ...current, canWriteFiles: event.target.checked }))} />
                <span>可写文件</span>
              </label>
              <label className="settings-check">
                <input type="checkbox" checked={agentDraft.canRunCommands} disabled={agentDraft.adapterType === "dummy"} onChange={(event) => setAgentDraft((current) => ({ ...current, canRunCommands: event.target.checked }))} />
                <span>可运行命令</span>
              </label>
              <label className="settings-check">
                <input type="checkbox" checked={agentDraft.enabled} onChange={(event) => setAgentDraft((current) => ({ ...current, enabled: event.target.checked }))} />
                <span>立即启用</span>
              </label>
            </div>
          </form>
          {state.app.agentError && <div className="settings-inline-error">{state.app.agentError}</div>}
        </SettingCard>
      </>
    );
  }

  function renderSlotEditor() {
    if (!slotDraft) {
      return null;
    }

    return (
      <form className="settings-slot-editor" onSubmit={(event) => {
        event.preventDefault();
        saveSlotDraft();
      }}>
        <div className="settings-form-grid">
          <label className="settings-form-field">
            <span>名称</span>
            <input value={slotDraft.name} onChange={(event) => setSlotDraft({ ...slotDraft, name: event.target.value })} />
          </label>
          <label className="settings-form-field">
            <span>类型</span>
            <select value={slotDraft.kind} onChange={(event) => setSlotDraft({ ...slotDraft, kind: event.target.value as TerminalSlot["kind"] })}>
              <option value="preview">预览</option>
              <option value="validation">验证</option>
            </select>
          </label>
          <label className="settings-form-field">
            <span>工作目录</span>
            <input value={slotDraft.cwd} placeholder="项目根目录" onChange={(event) => setSlotDraft({ ...slotDraft, cwd: event.target.value })} />
          </label>
          <label className="settings-form-field wide">
            <span>命令</span>
            <input value={slotDraft.command} placeholder="pnpm test" onChange={(event) => setSlotDraft({ ...slotDraft, command: event.target.value })} />
          </label>
        </div>
        <div className="settings-form-actions">
          <button type="button" className="settings-text-control" onClick={() => setSlotDraft(null)}>取消</button>
          <Button type="submit" variant="primary" disabled={!slotDraft.name.trim() || !slotDraft.command.trim()}>
            保存槽位
          </Button>
        </div>
      </form>
    );
  }

  function renderSafety() {
    return (
      <>
        <SettingCard title="项目终端槽位">
          {!project ? (
            <div className="settings-empty-state">打开项目后可配置预览和验证命令。</div>
          ) : (
            <>
              <div className="settings-card-toolbar">
                <div className="settings-row-desc">
                  槽位保存在 <code>.loom/terminal-slots.json</code>，供 Testing 驾驶舱和实施自动验证使用。
                </div>
                <div className="settings-heading-actions">
                  <Button type="button" variant="ghost" iconLeft={<RefreshCw size={14} />} onClick={() => void resetSlotsToDetected()}>
                    恢复检测结果
                  </Button>
                  <Button type="button" variant="primary" iconLeft={<Plus size={14} />} onClick={() => setSlotDraft(blankTerminalSlotDraft())}>
                    添加槽位
                  </Button>
                </div>
              </div>
              <div className="settings-table-wrap">
                <table className="settings-agent-table settings-command-table">
                  <thead>
                    <tr>
                      <th>名称</th>
                      <th>命令</th>
                      <th>类型</th>
                      <th>工作目录</th>
                      <th />
                    </tr>
                  </thead>
                  <tbody>
                    {slots.map((slot) => (
                      <tr key={slot.id}>
                        <td><strong>{slot.name}</strong></td>
                        <td><span className="settings-chip mono">{slot.command || "未配置"}</span></td>
                        <td><span className="settings-tag">{slot.kind === "preview" ? "预览" : "验证"}</span></td>
                        <td><span className="settings-chip mono">{resolveTerminalSlotCwd(project.path, slot)}</span></td>
                        <td>
                          <div className="settings-agent-actions">
                            <button type="button" className="settings-icon-control" title={`编辑 ${slot.name}`} onClick={() => setSlotDraft(draftFromTerminalSlot(slot))}>
                              <Pencil size={14} />
                            </button>
                            <button type="button" className="settings-icon-control danger" title={`删除 ${slot.name}`} onClick={() => removeSlot(slot)}>
                              <Trash2 size={14} />
                            </button>
                          </div>
                        </td>
                      </tr>
                    ))}
                    {slotsLoaded && slots.length === 0 && (
                      <tr>
                        <td colSpan={5}><span className="settings-dim">尚未配置槽位。</span></td>
                      </tr>
                    )}
                  </tbody>
                </table>
              </div>
              {renderSlotEditor()}
              {slotError && <div className="settings-inline-error">{slotError}</div>}
            </>
          )}
        </SettingCard>
        <SettingCard title="执行护栏">
          <Field
            title="确认依赖变更"
            description="安装依赖前提示；破坏性文件/Git 命令和生产环境目标始终需要一次性后端批准。"
            control={
              <ToggleControl
                checked={appSettings.confirmBeforeCommands}
                onChange={(checked) => updateAppSettings({ confirmBeforeCommands: checked })}
              />
            }
          />
          <Field
            title="命令超时"
            description="一次性验证命令超过该秒数后停止；预览终端会持续运行，直到手工停止。"
            control={
              <input
                className="settings-number-input"
                type="number"
                min={5}
                max={3600}
                value={appSettings.commandTimeoutSeconds}
                onChange={(event) => setAppSettings((current) => ({ ...current, commandTimeoutSeconds: Number(event.target.value) }))}
                onBlur={(event) => updateAppSettings({ commandTimeoutSeconds: Number(event.target.value) })}
              />
            }
          />
          <Field
            title="日志敏感信息脱敏"
            description="命令文本和输出在持久化前始终经过后端脱敏规则。"
            control={<span className="settings-tag ok">始终开启</span>}
          />
          {settingsError && <div className="settings-inline-error">{settingsError}</div>}
        </SettingCard>
      </>
    );
  }

  function renderAbout() {
    const resources = project
      ? [
          { label: "需求文档", path: `${project.path}/docs/requirements.md` },
          { label: "计划索引", path: `${project.path}/docs/PLANS.md` },
        ]
      : [];

    return (
      <>
        <SettingCard title="Loom">
          <div className="settings-about-head">
            <div className="settings-bigmark">L</div>
            <div>
              <h2>{health?.app ?? "Loom"}</h2>
              <p>多 Agent 本地开发工作台</p>
              <div className="settings-tag-row">
                <span className="settings-chip mono">v{health?.version ?? "未知"}</span>
                <span className="settings-chip mono">{health?.backend ?? "等待后端"}</span>
                <span className="settings-tag ok">{health?.status ?? "加载中"}</span>
              </div>
            </div>
          </div>
        </SettingCard>
        <SettingCard title="后端健康状态">
          <Field title="状态" description="由 Tauri health_check 命令报告。" control={<span className="settings-tag ok">{health?.status ?? "加载中"}</span>} />
          <Field title="最近检查" control={<span className="settings-chip mono">{shortTimestamp(health?.timestampMs)}</span>} />
        </SettingCard>
        <SettingCard title="诊断与支持">
          <Field
            title="脱敏诊断包"
            description="包含版本、系统、当前任务运行元数据和执行策略判定；不会写入原始项目路径或会话凭据。"
            control={
              <Button
                type="button"
                variant="ghost"
                iconLeft={<ShieldCheck size={14} />}
                disabled={!project || !hasTauriRuntime()}
                onClick={() => void exportProjectDiagnostics()}
              >
                导出诊断包
              </Button>
            }
          />
          <Field
            title="日志范围"
            description="日志默认不导出；勾选后仅包含当前任务最近的脱敏日志尾部。"
            control={
              <ToggleControl checked={includeDiagnosticLogs} onChange={setIncludeDiagnosticLogs} />
            }
          />
          {diagnosticNotice && <div className="settings-row-desc">{diagnosticNotice}</div>}
        </SettingCard>
        <SettingCard title="资源">
          {resources.length === 0 ? (
            <div className="settings-empty-state">打开项目后可查看工作区内的本地文档。</div>
          ) : resources.map((item) => (
            <button
              type="button"
              className="settings-link-row"
              disabled={!hasTauriRuntime()}
              onClick={() => openResource(item.path)}
              key={item.path}
            >
              <span>{item.label}</span>
              <ExternalLink size={13} />
            </button>
          ))}
        </SettingCard>
      </>
    );
  }

  const body = {
    general: renderGeneral,
    appearance: renderAppearance,
    agents: renderAgents,
    safety: renderSafety,
    about: renderAbout,
  }[activeTab]();

  return (
    <div className="settings-page">
      <div className="settings-topbar" data-tauri-drag-region>
        <div className="settings-topbar-spacer" data-tauri-drag-region />
      </div>

      <main className="settings-main">
        <div className="settings-columns">
          <nav className="settings-nav">
            <button type="button" className="settings-nav-back" aria-label="返回" onClick={onBack}>
              <span className="settings-nav-icon">
                <ArrowLeft size={15} />
              </span>
              设置
            </button>
            <div className="settings-nav-items">
              {settingsTabs.map((tab) => (
                <button
                  type="button"
                  className={`settings-nav-item ${tab.id === activeTab ? "active" : ""}`}
                  onClick={() => setActiveTab(tab.id)}
                  key={tab.id}
                >
                  <span className="settings-nav-icon">{tab.icon}</span>
                  {tab.label}
                </button>
              ))}
            </div>
          </nav>

          <div className="settings-body">{body}</div>
        </div>
      </main>
    </div>
  );
}

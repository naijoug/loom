import { useCallback, useEffect, useRef, useState } from "react";
import { chatReadRunLogs } from "../../api/chatClient";
import type { ChatTurn } from "../../domain/chat";

const labels = { starting: "准备启动", running: "运行中", cancelling: "停止中", completed: "已完成",
  failed: "失败", cancelled: "已停止", timed_out: "超时", interrupted: "已中断" };

export function ChatRunDetails({ projectPath, sessionId, turn, readLogs = chatReadRunLogs }: {
  projectPath: string; sessionId: string; turn: ChatTurn; readLogs?: typeof chatReadRunLogs;
}) {
  const [open, setOpen] = useState(false);
  const [stream, setStream] = useState<"stdout" | "stderr">("stdout");
  const [text, setText] = useState("");
  const [offset, setOffset] = useState(0);
  const [more, setMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const generation = useRef(0);
  const load = useCallback(async (cursor: number) => {
    const request = ++generation.current;
    setLoading(true); setError(null);
    if (cursor === 0) { setText(""); setOffset(0); setMore(false); }
    try {
      const page = await readLogs(projectPath, sessionId, turn.id, stream, cursor);
      if (generation.current !== request) return;
      setText((previous) => cursor === 0 ? page.text : previous + page.text);
      setOffset(page.nextOffset); setMore(page.hasMore);
    } catch (err) { if (generation.current === request) setError(String(err)); }
    finally { if (generation.current === request) setLoading(false); }
  }, [projectPath, sessionId, turn.id, stream, readLogs]);
  useEffect(() => {
    if (open) void load(0);
    return () => { generation.current++; };
  }, [open, load]);
  const elapsed = turn.finishedAtMs == null ? null : Math.max(0, turn.finishedAtMs - (turn.startedAtMs ?? turn.acceptedAtMs));
  return <div className="chat-run-details">
    <button type="button" className="chat-link-btn" aria-expanded={open} onClick={() => setOpen(!open)}>
      {labels[turn.status]} · 执行记录{elapsed == null ? "" : ` · ${(elapsed / 1000).toFixed(1)} 秒`}
    </button>
    {open && <section aria-label="回合执行记录" className="chat-run-panel">
      <dl>
        <dt>程序</dt><dd><code>{turn.invocation.program}</code></dd>
        <dt>参数（脱敏）</dt><dd><pre>{JSON.stringify(turn.invocation.args, null, 2)}</pre></dd>
        <dt>工作目录</dt><dd><code>{turn.invocation.cwd}</code></dd>
        <dt>权限 / Agent</dt><dd>{turn.invocation.permissionMode} / {turn.invocation.agentId}</dd>
        <dt>接收时间</dt><dd>{new Date(turn.acceptedAtMs).toLocaleString()}</dd>
        <dt>启动时间</dt><dd>{turn.startedAtMs == null ? "未启动" : new Date(turn.startedAtMs).toLocaleString()}</dd>
        <dt>结束时间</dt><dd>{turn.finishedAtMs == null ? "尚未结束" : new Date(turn.finishedAtMs).toLocaleString()}</dd>
        <dt>退出码</dt><dd>{turn.exitCode ?? "无退出码"}</dd>
        {turn.terminationReason && <><dt>结束原因</dt><dd>{turn.terminationReason}</dd></>}
      </dl>
      {turn.errorSummary && <p role="alert">{turn.errorSummary}</p>}
      <div className="chat-run-controls" role="group" aria-label="执行日志">
        <button type="button" aria-pressed={stream === "stdout"} onClick={() => setStream("stdout")}>stdout</button>
        <button type="button" aria-pressed={stream === "stderr"} onClick={() => setStream("stderr")}>stderr</button>
        <button type="button" disabled={loading} onClick={() => void load(0)}>重新加载</button>
      </div>
      {error && <p role="alert">{error}</p>}
      <pre className="chat-run-log" aria-label={`${stream} 日志`}>{text || (loading ? "读取中…" : error ? "日志未能读取" : "暂无输出")}</pre>
      {more && <button type="button" disabled={loading} onClick={() => void load(offset)}>{loading ? "读取中…" : "加载更多"}</button>}
    </section>}
  </div>;
}

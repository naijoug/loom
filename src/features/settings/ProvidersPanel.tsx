import { useEffect, useRef, useState } from "react";
import { Eye, EyeOff } from "lucide-react";
import type { ModelProviderTestResult } from "../../domain";
import { useT } from "../../i18n";
import { useAgentCatalog } from "../../hooks/useAgentCatalog";
import { useAppState } from "../../state/AppStateContext";
import {
  draftFromPreset,
  draftFromProvider,
  isRevisionConflict,
  keySourceLabel,
  PROVIDER_KINDS,
  PROVIDER_PRESETS,
  providerInput,
  providerKindKey,
  providerTestLabel,
  providerUsers,
  type ProviderDraft,
} from "./providerDraft";

/**
 * Settings → Model sources. Keys go in through a password field and are dropped from state once saved;
 * the list only ever shows where a key comes from.
 * `adding` is driven by the page header's Add button.
 */
export function ProvidersPanel({
  adding,
  onAddingChange,
}: {
  adding: boolean;
  onAddingChange: (open: boolean) => void;
}) {
  const t = useT();
  const { state } = useAppState();
  const { loadAgents, diagnoseAgents, loadProviders, saveProvider, deleteProvider, testProvider } = useAgentCatalog();
  const { revision, providers } = state.providers;
  const [draft, setDraft] = useState<ProviderDraft | null>(null);
  const [presetId, setPresetId] = useState<string | null>(null);
  const [showKey, setShowKey] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [conflict, setConflict] = useState(false);
  const [tests, setTests] = useState<Record<string, ModelProviderTestResult | "running">>({});
  // The header's Add button replaces an open edit form.
  useEffect(() => {
    if (adding) setDraft((current) => (current?.id ? null : current));
  }, [adding]);
  const form = draft ?? (adding ? draftFromPreset(PROVIDER_PRESETS[3]) : null);
  // The form opens below the list; bring it into view when it opens or switches provider.
  const formRef = useRef<HTMLFormElement>(null);
  const formKey = form ? (form.id ?? "new") : null;
  useEffect(() => {
    if (formKey) formRef.current?.scrollIntoView?.({ block: "nearest" });
  }, [formKey]);
  const update = (patch: Partial<ProviderDraft>) => form && setDraft({ ...form, ...patch });
  function open(next: ProviderDraft | null, preset: string | null = null) {
    setDraft(next);
    setPresetId(preset);
    setShowKey(false);
    setConflict(false);
    setError("");
    onAddingChange(!!next && next.id === null);
  }
  async function failed(reason: unknown) {
    if (isRevisionConflict(reason)) {
      setConflict(true);
      await loadProviders().catch((error) => setError(String(error)));
    } else setError(String(reason));
  }
  // An api agent's availability follows its provider's key, so refresh the Agents after each change.
  async function refreshAgents() {
    await loadAgents();
    await diagnoseAgents().catch(() => undefined);
  }
  async function runTest(id: string) {
    setTests((all) => ({ ...all, [id]: "running" }));
    try {
      const result = await testProvider(id);
      setTests((all) => ({ ...all, [id]: result }));
    } catch (reason) {
      setTests(({ [id]: _, ...rest }) => rest);
      setError(String(reason));
    }
  }
  async function submit(event: { preventDefault(): void }, test = false) {
    event.preventDefault();
    if (!form || busy) return;
    setBusy(true);
    setError("");
    setConflict(false);
    try {
      const input = providerInput(
        form,
        revision,
        providers.map((item) => item.id),
      );
      await saveProvider(input);
      open(null);
      void refreshAgents();
      if (test) await runTest(input.id);
    } catch (reason) {
      await failed(reason);
    } finally {
      setBusy(false);
    }
  }
  async function remove(id: string) {
    if (!window.confirm(t("removeProviderWarning"))) return;
    setBusy(true);
    setError("");
    try {
      await deleteProvider(id, revision);
      void refreshAgents();
    } catch (reason) {
      await failed(reason);
    } finally {
      setBusy(false);
    }
  }
  const presets = (
    <div className="settings-presets" role="radiogroup" aria-label={t("providerPreset")}>
      {PROVIDER_PRESETS.map((preset) => (
        <button
          type="button"
          role="radio"
          key={preset.id}
          aria-checked={presetId === preset.id}
          onClick={() => open(draftFromPreset(preset), preset.id)}
        >
          {t(preset.label)}
        </button>
      ))}
    </div>
  );
  const existing = form?.id ? providers.find((item) => item.id === form.id) : undefined;
  return (
    <>
      {error && (
        <p role="alert" className="chat-error">
          {error}
        </p>
      )}
      {conflict && (
        <p role="status" className="settings-banner">
          {t("providerConflict")}
        </p>
      )}
      {providers.length === 0 && !form ? (
        <section className="settings-card">
          <p>{t("providersEmpty")}</p>
          {presets}
        </section>
      ) : (
        providers.length > 0 && (
          <div className="settings-table-wrap settings-providers">
            <table>
              <thead>
                <tr>
                  <th>{t("name")}</th>
                  <th>{t("providerKindAndUrl")}</th>
                  <th>{t("providerDefaultModel")}</th>
                  <th>{t("providerKey")}</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {providers.map((provider) => {
                  const users = providerUsers(state.agents, provider.id);
                  const names = users.map((agent) => agent.name).join(", ");
                  const key = keySourceLabel(provider, t);
                  const test = tests[provider.id];
                  return (
                    <tr key={provider.id} data-provider={provider.id}>
                      <td className="settings-provider-name">
                        <strong>{provider.name}</strong>
                        {users.length > 0 && <small>{t("providerUsedBy", { agents: names })}</small>}
                      </td>
                      <td className="settings-provider-url">
                        {t(providerKindKey(provider.kind))}
                        <small>
                          <code>{provider.baseUrl}</code>
                        </small>
                      </td>
                      <td className="settings-provider-model">
                        <code>{provider.model ?? "—"}</code>
                      </td>
                      <td className="settings-provider-key">
                        <span className="settings-status" data-tone={key.tone}>
                          <span className="settings-key-dot" aria-hidden="true" />
                          {key.text}
                        </span>
                        {test && (
                          <small role="status" data-outcome={test === "running" ? "running" : test.outcome}>
                            {test === "running" ? t("loading") : providerTestLabel(test, t)}
                          </small>
                        )}
                      </td>
                      <td className="settings-row-actions">
                        <button type="button" onClick={() => open(draftFromProvider(provider))}>
                          {t("edit")}
                        </button>
                        <button
                          type="button"
                          disabled={busy || test === "running"}
                          onClick={() => void runTest(provider.id)}
                        >
                          {t("test")}
                        </button>
                        <button
                          type="button"
                          disabled={busy || users.length > 0}
                          title={users.length > 0 ? t("providerInUse", { agents: names }) : undefined}
                          onClick={() => void remove(provider.id)}
                        >
                          {t("remove")}
                        </button>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )
      )}
      {form && (
        <form
          ref={formRef}
          className="settings-card settings-agent-form settings-provider-form"
          onSubmit={(event) => void submit(event)}
        >
          <h3>{form.id ? t("editProviderTitle", { name: existing?.name ?? form.name }) : t("addProviderTitle")}</h3>
          {!form.id && (
            <div className="settings-field">
              <span>{t("providerPreset")}</span>
              {presets}
            </div>
          )}
          <label>
            {t("name")}
            <input
              required
              maxLength={64}
              value={form.name}
              onChange={(event) => update({ name: event.target.value })}
            />
          </label>
          <div className="settings-field" role="radiogroup" aria-label={t("providerKind")}>
            <span>{t("providerKind")}</span>
            <div className="settings-choices settings-kinds">
              {PROVIDER_KINDS.map((kind) => (
                <button
                  type="button"
                  role="radio"
                  key={kind}
                  aria-checked={form.kind === kind}
                  onClick={() => update({ kind })}
                >
                  <span className="settings-ring" aria-hidden="true" />
                  {t(providerKindKey(kind))}
                </button>
              ))}
            </div>
          </div>
          <label>
            {t("providerBaseUrl")}
            <input
              required
              type="url"
              spellCheck={false}
              value={form.baseUrl}
              placeholder={form.placeholder || undefined}
              onChange={(event) => update({ baseUrl: event.target.value })}
            />
            <small>{t("providerBaseUrlHint")}</small>
          </label>
          <label>
            {t("providerDefaultModel")}
            <input
              spellCheck={false}
              maxLength={128}
              value={form.model}
              onChange={(event) => update({ model: event.target.value })}
            />
            <small>{t("providerModelHint")}</small>
          </label>
          <div className="settings-field">
            <span>{t("providerApiKey")}</span>
            {form.key.action === "set" ? (
              <div className="settings-key-input">
                <input
                  type={showKey ? "text" : "password"}
                  autoComplete="off"
                  spellCheck={false}
                  aria-label={t("providerApiKey")}
                  value={form.key.value}
                  onChange={(event) => update({ key: { action: "set", value: event.target.value } })}
                />
                <button
                  type="button"
                  className="chat-icon-button"
                  aria-label={t("providerShowKey")}
                  aria-pressed={showKey}
                  onClick={() => setShowKey(!showKey)}
                >
                  {showKey ? <EyeOff size={14} aria-hidden="true" /> : <Eye size={14} aria-hidden="true" />}
                </button>
                {form.id && (
                  <button type="button" onClick={() => update({ key: { action: "keep" } })}>
                    {t("cancel")}
                  </button>
                )}
              </div>
            ) : (
              <div className="settings-key-state">
                <span>
                  {form.key.action === "clear"
                    ? t("providerKeyCleared")
                    : existing?.storedKey
                      ? t("providerKeySaved")
                      : t("providerKeyNone")}
                </span>
                <button type="button" onClick={() => update({ key: { action: "set", value: "" } })}>
                  {t("providerKeyReplace")}
                </button>
                {existing?.storedKey && form.key.action === "keep" && (
                  <button
                    type="button"
                    onClick={() => window.confirm(t("providerKeyClearConfirm")) && update({ key: { action: "clear" } })}
                  >
                    {t("providerKeyClear")}
                  </button>
                )}
              </div>
            )}
          </div>
          <label>
            {t("providerKeyEnv")}
            <input
              spellCheck={false}
              maxLength={128}
              pattern="[A-Za-z_][A-Za-z0-9_]*"
              placeholder="DEEPSEEK_API_KEY"
              value={form.keyEnv}
              onChange={(event) => update({ keyEnv: event.target.value })}
            />
            <small>{t("providerKeyEnvHint")}</small>
          </label>
          <div className="settings-form-actions">
            <button type="button" onClick={() => open(null)}>
              {t("cancel")}
            </button>
            <button type="submit" disabled={busy}>
              {t("save")}
            </button>
            <button
              type="button"
              className="settings-primary"
              disabled={busy}
              onClick={(event) => {
                const target = event.currentTarget.form;
                if (target?.reportValidity()) void submit(event, true);
              }}
            >
              {t("saveAndTest")}
            </button>
          </div>
        </form>
      )}
    </>
  );
}

import type {
  AgentConfig,
  ModelProvider,
  ModelProviderInput,
  ModelProviderKind,
  ModelProviderTestResult,
} from "../../domain";
import type { MessageKey } from "../../i18n/en";

type Translate = (key: MessageKey, values?: Record<string, string | number>) => string;

/** Presets only prefill the form; every field stays editable. */
export interface ProviderPreset {
  id: string;
  label: MessageKey;
  name: string;
  kind: ModelProviderKind;
  baseUrl: string;
  placeholder: string;
}
export const PROVIDER_PRESETS: ProviderPreset[] = [
  {
    id: "anthropic",
    label: "presetAnthropic",
    name: "Anthropic",
    kind: "anthropic",
    baseUrl: "https://api.anthropic.com",
    placeholder: "https://api.anthropic.com",
  },
  {
    id: "openai",
    label: "presetOpenai",
    name: "OpenAI",
    kind: "openai-responses",
    baseUrl: "https://api.openai.com/v1",
    placeholder: "https://api.openai.com/v1",
  },
  {
    id: "openrouter",
    label: "presetOpenrouter",
    name: "OpenRouter",
    kind: "openai-chat",
    baseUrl: "https://openrouter.ai/api/v1",
    placeholder: "https://openrouter.ai/api/v1",
  },
  {
    id: "openai-compatible",
    label: "presetOpenaiCompatible",
    name: "",
    kind: "openai-chat",
    baseUrl: "",
    placeholder: "https://example.com/v1",
  },
  {
    id: "anthropic-compatible",
    label: "presetAnthropicCompatible",
    name: "",
    kind: "anthropic",
    baseUrl: "",
    placeholder: "https://example.com/anthropic",
  },
  {
    id: "local-gateway",
    label: "presetLocalGateway",
    name: "Local gateway",
    kind: "openai-chat",
    baseUrl: "http://127.0.0.1:",
    placeholder: "http://127.0.0.1:<port>/v1",
  },
];
export const PROVIDER_KINDS: ModelProviderKind[] = ["anthropic", "openai-responses", "openai-chat"];

export function providerKindKey(kind: ModelProviderKind): MessageKey {
  return kind === "anthropic"
    ? "providerKindAnthropic"
    : kind === "openai-responses"
      ? "providerKindOpenai-responses"
      : "providerKindOpenai-chat";
}

/** `id` is null until the provider is first saved. The key lives here only while the form is open. */
export interface ProviderDraft {
  id: string | null;
  name: string;
  kind: ModelProviderKind;
  baseUrl: string;
  model: string;
  keyEnv: string;
  key: ModelProviderInput["key"];
  placeholder: string;
}
export function draftFromPreset(preset: ProviderPreset): ProviderDraft {
  return {
    id: null,
    name: preset.name,
    kind: preset.kind,
    baseUrl: preset.baseUrl,
    model: "",
    keyEnv: "",
    key: { action: "set", value: "" },
    placeholder: preset.placeholder,
  };
}
export function draftFromProvider(provider: ModelProvider): ProviderDraft {
  return {
    id: provider.id,
    name: provider.name,
    kind: provider.kind,
    baseUrl: provider.baseUrl,
    model: provider.model ?? "",
    keyEnv: provider.keyEnv ?? "",
    key: { action: "keep" },
    placeholder: "",
  };
}

/** Lowercase ASCII slug of the name, made unique among `taken`; names without ASCII letters become `provider`. */
export function newProviderId(name: string, taken: string[]): string {
  const base =
    name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 56) || "provider";
  let id = base;
  for (let n = 2; taken.includes(id); n++) id = `${base}-${n}`;
  return id;
}

/** An empty new key means "no key"; the backend rejects an empty one. */
export function providerInput(draft: ProviderDraft, revision: number, taken: string[]): ModelProviderInput {
  const key = draft.key.action === "set" && !draft.key.value.trim() ? { action: "keep" as const } : draft.key;
  return {
    id: draft.id ?? newProviderId(draft.name, taken),
    expectedRevision: revision,
    name: draft.name.trim(),
    kind: draft.kind,
    baseUrl: draft.baseUrl.trim(),
    model: draft.model.trim() || null,
    key: key.action === "set" ? { action: "set", value: key.value.trim() } : key,
    keyEnv: draft.keyEnv.trim() || null,
  };
}

/** Where the key comes from, never the key: ok has one, none needs none, warn is missing one. */
export function keySourceLabel(provider: ModelProvider, t: Translate): { tone: "ok" | "none" | "warn"; text: string } {
  if (provider.keySource === "environment") return { tone: "ok", text: `$${provider.keyEnv}` };
  if (provider.keySource === "stored") return { tone: "ok", text: t("providerKeyStored") };
  if (provider.configured) return { tone: "none", text: t("providerKeyLocal") };
  if (provider.keyEnv) return { tone: "warn", text: t("providerKeyEnvMissing", { name: `$${provider.keyEnv}` }) };
  return { tone: "warn", text: t("providerKeyMissing") };
}

export function providerTestLabel(result: ModelProviderTestResult, t: Translate): string {
  const status = result.httpStatus ?? "?";
  switch (result.outcome) {
    case "ok":
      return t("providerTestOk");
    case "key_missing":
      return t("providerTestKey_missing");
    case "auth_failed":
      return t("providerTestAuth_failed", { status });
    case "not_found":
      return t("providerTestNot_found");
    case "rate_limited":
      return t("providerTestRate_limited");
    case "http_error":
      return t("providerTestHttp_error", { status });
    case "invalid_response":
      return t("providerTestInvalid_response");
    case "timeout":
      return t("providerTestTimeout");
    default:
      return t("providerTestUnreachable");
  }
}

/** Host and port of a base URL, short enough for a table cell; the full URL goes in the title. */
export function providerHost(baseUrl: string | undefined): string {
  if (!baseUrl) return "—";
  try {
    return new URL(baseUrl).host;
  } catch {
    return baseUrl;
  }
}

export function providerUsers(agents: AgentConfig[], providerId: string): AgentConfig[] {
  return agents.filter((agent) => agent.providerId === providerId);
}

/** The backend reports this message when another writer saved first. */
export function isRevisionConflict(error: unknown): boolean {
  return String(error).includes("Provider settings changed");
}

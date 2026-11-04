import { useEffect, useState } from "react";
import { LoaderCircle, RefreshCw, TrendingUp } from "lucide-react";
import type { LearnApi, TrendingPeriod, TrendingResult } from "../../api/learn";
import { ErrorText } from "../../components/common/ErrorText";
import { useT } from "../../i18n";
import { RepoCard, sameRepoUrl } from "./RepoCard";

const LANGUAGES = ["Python", "TypeScript", "Rust", "Go", "JavaScript", "Java", "C++", "C", "Swift", "Kotlin", "Zig"];
const PERIODS: TrendingPeriod[] = ["day", "week", "month"];
const PERIOD_LABEL = { day: "learnTrendingDay", week: "learnTrendingWeek", month: "learnTrendingMonth" } as const;

function stored(key: string, fallback: string) {
  try {
    return localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
}
function store(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* per-device convenience only */
  }
}

/** Rising repositories from GitHub search (created within the period, by stars). */
export function TrendingList({
  api,
  selectedUrl,
  onStudy,
}: {
  api: LearnApi;
  selectedUrl: string;
  onStudy: (url: string) => void;
}) {
  const t = useT();
  const [period, setPeriod] = useState<TrendingPeriod>(() => {
    const value = stored("loom-learn-trending-period", "week");
    return PERIODS.includes(value as TrendingPeriod) ? (value as TrendingPeriod) : "week";
  });
  const [language, setLanguage] = useState(() => {
    const value = stored("loom-learn-trending-language", "");
    return LANGUAGES.includes(value) ? value : "";
  });
  const [refresh, setRefresh] = useState(0);
  const [result, setResult] = useState<TrendingResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    store("loom-learn-trending-period", period);
    store("loom-learn-trending-language", language);
    let disposed = false;
    setLoading(true);
    setError("");
    api
      .trending(period, language || undefined, refresh > 0)
      .then((next) => {
        if (!disposed) setResult(next);
      })
      .catch((reason) => {
        if (!disposed) setError(String(reason));
      })
      .finally(() => {
        if (!disposed) setLoading(false);
      });
    return () => {
      disposed = true;
    };
  }, [api, period, language, refresh]);
  return (
    <section className="learn-trending" aria-labelledby="learn-trending-title" aria-busy={loading}>
      <header className="learn-trending-head">
        <div>
          <h2 id="learn-trending-title">
            <TrendingUp size={16} aria-hidden="true" />
            {t("learnTrendingTitle")}
          </h2>
          <p className="learn-note" title={result?.query}>
            {t(`learnTrendingCaption_${period}`)}
          </p>
        </div>
        <div className="learn-trending-filters">
          <div className="learn-segmented" role="radiogroup" aria-label={t("learnTrendingPeriod")}>
            {PERIODS.map((value) => (
              <button
                key={value}
                type="button"
                role="radio"
                aria-checked={period === value}
                onClick={() => setPeriod(value)}
              >
                {t(PERIOD_LABEL[value])}
              </button>
            ))}
          </div>
          <select
            aria-label={t("learnTrendingLanguage")}
            value={language}
            onChange={(event) => setLanguage(event.target.value)}
          >
            <option value="">{t("learnTrendingAllLanguages")}</option>
            {LANGUAGES.map((value) => (
              <option key={value} value={value}>
                {value}
              </option>
            ))}
          </select>
          <button
            type="button"
            className="chat-icon-button"
            aria-label={t("learnTrendingRefresh")}
            title={t("learnTrendingRefresh")}
            disabled={loading}
            onClick={() => setRefresh((value) => value + 1)}
          >
            {loading ? <LoaderCircle size={14} className="chat-spin" /> : <RefreshCw size={14} />}
          </button>
        </div>
      </header>
      {error && <ErrorText text={error} alert className="learn-error" />}
      {!result && loading && <p className="learn-note">{t("learnTrendingLoading")}</p>}
      {result && !result.repos.length && !loading && <p className="learn-note">{t("learnTrendingEmpty")}</p>}
      {result && result.repos.length > 0 && (
        <div className="learn-repo-grid" data-stale={loading || undefined}>
          {result.repos.map((repo) => (
            <RepoCard key={repo.fullName} repo={repo} selected={sameRepoUrl(selectedUrl, repo.url)} onStudy={onStudy} />
          ))}
        </div>
      )}
    </section>
  );
}

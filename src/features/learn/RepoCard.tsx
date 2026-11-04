import { ExternalLink, GraduationCap, Star } from "lucide-react";
import type { GithubRepoCard } from "../../api/learn";
import { openExternalLink } from "../../components/externalLink";
import { useT } from "../../i18n";

/** 1234 → "1.2k", 1_250_000 → "1.3m". */
export function compactCount(value: number): string {
  if (value < 1000) return String(value);
  if (value < 1_000_000) return `${(value / 1000).toFixed(value < 10_000 ? 1 : 0).replace(/\.0$/, "")}k`;
  return `${(value / 1_000_000).toFixed(1).replace(/\.0$/, "")}m`;
}

/** Compares repository URLs ignoring case, a trailing slash and `.git`. */
export function sameRepoUrl(a: string, b: string): boolean {
  const norm = (url: string) =>
    url
      .trim()
      .toLowerCase()
      .replace(/\/+$/, "")
      .replace(/\.git$/, "");
  return !!a.trim() && norm(a) === norm(b);
}

/** A GitHub repository with "Study" (fills the new-study form) and "Open on GitHub". */
export function RepoCard({
  repo,
  selected,
  compact,
  onStudy,
}: {
  repo: GithubRepoCard;
  selected: boolean;
  compact?: boolean;
  onStudy: (url: string) => void;
}) {
  const t = useT();
  return (
    <article className="learn-repo" data-selected={selected || undefined} data-compact={compact || undefined}>
      <header>
        <button
          type="button"
          className="learn-repo-name"
          title={repo.fullName}
          onClick={() => openExternalLink(repo.url)}
        >
          <span className="learn-repo-owner">{repo.owner}/</span>
          <strong>{repo.name}</strong>
        </button>
        {repo.archived && <span className="learn-repo-badge">{t("learnRepoArchived")}</span>}
      </header>
      {repo.description && <p className="learn-repo-desc">{repo.description}</p>}
      {!compact && repo.topics.length > 0 && (
        <p className="learn-repo-topics">
          {repo.topics.slice(0, 4).map((topic) => (
            <span key={topic}>{topic}</span>
          ))}
        </p>
      )}
      <footer>
        <span className="learn-repo-meta">
          {repo.language && (
            <span className="learn-repo-lang">
              <span className="learn-repo-lang-dot" aria-hidden="true" />
              {repo.language}
            </span>
          )}
          <span title={t("learnRepoStars", { count: repo.stars })}>
            <Star size={12} aria-hidden="true" />
            {compactCount(repo.stars)}
          </span>
        </span>
        <span className="learn-repo-actions">
          <button
            type="button"
            className="chat-icon-button"
            aria-label={t("learnOpenGithub")}
            title={t("learnOpenGithub")}
            onClick={() => openExternalLink(repo.url)}
          >
            <ExternalLink size={13} />
          </button>
          <button
            type="button"
            className="learn-repo-study"
            aria-pressed={selected}
            aria-label={`${t("learnRepoStudy")} ${repo.fullName}`}
            onClick={() => onStudy(repo.url)}
          >
            <GraduationCap size={13} aria-hidden="true" />
            {selected ? t("learnRepoSelected") : t("learnRepoStudy")}
          </button>
        </span>
      </footer>
    </article>
  );
}

import { ArrowLeft, Moon, Sun } from "lucide-react";
import { useTheme } from "../../contexts/ThemeContext";
import "./SettingsPage.css";

interface SettingsPageProps {
  onBack: () => void;
}

export function SettingsPage({ onBack }: SettingsPageProps) {
  const { theme, toggleTheme } = useTheme();

  return (
    <div className="settings-page">
      <aside className="settings-sidebar">
        <button
          type="button"
          className="settings-back-link"
          aria-label="Back"
          title="Back"
          onClick={onBack}
        >
          <ArrowLeft size={14} />
        </button>

        <nav className="settings-nav">
          <div className="settings-nav-item active">
            {theme === "dark" ? <Moon size={15} /> : <Sun size={15} />}
            Appearance
          </div>
        </nav>
      </aside>

      <main className="settings-main">
        <div className="settings-content">
          <h1 className="settings-title">Appearance</h1>

          <section className="settings-section">
            <h2 className="settings-section-title">Theme</h2>
            <div className="settings-card">
              <div className="settings-card-row split">
                <div className="settings-row-copy">
                  <div className="settings-row-title">Color mode</div>
                  <div className="settings-row-desc">Switch between light and dark UI.</div>
                </div>
                <button
                  type="button"
                  className="settings-icon-control"
                  aria-label={`Switch to ${theme === "dark" ? "light" : "dark"} theme`}
                  title={`Switch to ${theme === "dark" ? "light" : "dark"} theme`}
                  onClick={toggleTheme}
                >
                  {theme === "dark" ? <Moon size={16} /> : <Sun size={16} />}
                </button>
              </div>
            </div>
          </section>
        </div>
      </main>
    </div>
  );
}

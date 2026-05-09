import { Brand } from "./Brand";
import { Navigation } from "./Navigation";
import { useTheme } from "../../contexts/ThemeContext";

export function Sidebar() {
  const { toggleTheme, theme } = useTheme();

  return (
    <>
      <Brand />
      <Navigation />
      
      {/* Temporary theme toggle placed at the bottom of sidebar for testing */}
      <div style={{ padding: "16px", marginTop: "auto" }}>
        <button 
          type="button" 
          onClick={toggleTheme}
          style={{
            width: "100%",
            background: "var(--theme-accent)",
            color: "#fff",
            border: "none",
            padding: "8px 12px",
            borderRadius: 4,
            cursor: "pointer"
          }}
        >
          Toggle Theme ({theme})
        </button>
      </div>
    </>
  );
}

import "./Sidebar.css";

// Empty top spacer that reserves room for the overlaid macOS traffic lights and
// keeps the sidebar's content baseline aligned with the main header. The
// collapse/expand control lives in the header (see AppLayout).
export function Brand() {
  return <div className="sidebar-brand" data-tauri-drag-region />;
}

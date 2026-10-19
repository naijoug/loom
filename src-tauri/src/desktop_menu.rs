//! Tauri 2's native macOS Quit item can bypass ExitRequested. Route the menu
//! accelerator through AppHandle::exit so both runtime drain gates participate.
//! Dock/system termination remains a separate platform acceptance case.
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    AppHandle,
};
pub(crate) const QUIT_ID: &str = "loom-graceful-quit";
pub(crate) fn build<R: tauri::Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::default(app)?;
    let items = menu.items()?;
    let application = items
        .first()
        .and_then(|item| item.as_submenu())
        .ok_or_else(|| std::io::Error::other("default application menu is missing"))?;
    let children = application.items()?;
    let index = children
        .len()
        .checked_sub(1)
        .ok_or_else(|| std::io::Error::other("application menu is empty"))?;
    let quit = children[index]
        .as_predefined_menuitem()
        .ok_or_else(|| std::io::Error::other("expected native Quit menu item"))?;
    let label = quit.text()?;
    if label != PredefinedMenuItem::quit(app, None)?.text()? {
        return Err(std::io::Error::other("default Quit menu layout changed").into());
    }
    let replacement = MenuItem::with_id(app, QUIT_ID, label, true, Some("Cmd+Q"))?;
    application.remove_at(index)?;
    application.insert(&replacement, index)?;
    Ok(menu)
}

use crate::Result;

use super::BrowserManager;

impl BrowserManager {
    /// Move the mouse to the specified coordinates.
    pub async fn move_mouse(&self, x: f64, y: f64) -> Result<()> {
        let page = self.get_or_create_page().await?;
        page.move_mouse(x, y).await
    }

    /// Move the mouse by relative offset from current position.
    pub async fn move_mouse_relative(&self, dx: f64, dy: f64) -> Result<(f64, f64)> {
        let page = self.get_or_create_page().await?;
        page.move_mouse_relative(dx, dy).await
    }

    /// Click at the specified coordinates.
    pub async fn click(&self, x: f64, y: f64) -> Result<()> {
        let page = self.get_or_create_page().await?;
        page.click(x, y).await
    }

    /// Click at the current mouse position.
    pub async fn click_at_current(&self) -> Result<(f64, f64)> {
        let page = self.get_or_create_page().await?;
        page.click_at_current().await
    }

    /// Perform mouse down at the current position.
    pub async fn mouse_down_at_current(&self) -> Result<(f64, f64)> {
        let page = self.get_or_create_page().await?;
        page.mouse_down_at_current().await
    }

    /// Perform mouse up at the current position.
    pub async fn mouse_up_at_current(&self) -> Result<(f64, f64)> {
        let page = self.get_or_create_page().await?;
        page.mouse_up_at_current().await
    }

    /// Type text into the currently focused element.
    pub async fn type_text(&self, text: &str) -> Result<()> {
        let page = self.get_or_create_page().await?;
        page.type_text(text).await
    }

    /// Press a key (e.g., "Enter", "Tab", "Escape", "ArrowDown").
    pub async fn press_key(&self, key: &str) -> Result<()> {
        let page = self.get_or_create_page().await?;
        page.press_key(key).await
    }

    /// Scroll the page by the given delta in pixels.
    pub async fn scroll_by(&self, dx: f64, dy: f64) -> Result<()> {
        let page = self.get_or_create_page().await?;
        page.scroll_by(dx, dy).await
    }

    /// Navigate browser history backward one entry.
    pub async fn history_back(&self) -> Result<()> {
        let page = self.get_or_create_page().await?;
        page.go_back().await
    }

    /// Navigate browser history forward one entry.
    pub async fn history_forward(&self) -> Result<()> {
        let page = self.get_or_create_page().await?;
        page.go_forward().await
    }

    /// Get the current cursor position.
    pub async fn get_cursor_position(&self) -> Result<(f64, f64)> {
        let page = self.get_or_create_page().await?;
        page.get_cursor_position().await
    }
}


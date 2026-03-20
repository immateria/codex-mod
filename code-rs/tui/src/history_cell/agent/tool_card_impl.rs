impl crate::chatwidget::tool_cards::ToolCardCell for AgentRunCell {
    fn tool_card_key(&self) -> Option<&str> {
        self.cell_key()
    }

    fn set_tool_card_key(&mut self, key: Option<String>) {
        self.set_cell_key(key);
    }
}
const HEADING_INDENT: usize = 1;
const CONTENT_INDENT: usize = 3;

use super::*;

impl ChatComposer {
    pub(crate) fn set_subagent_commands(&mut self, mut names: Vec<String>) {
        names.retain(|n| !is_reserved_subagent_name(n));
        names.sort();
        self.subagent_commands = names;
        if let ActivePopup::Command(popup) = &mut self.active_popup {
            popup.set_subagent_commands(self.subagent_commands.clone());
        }
    }
}


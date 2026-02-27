use super::super::prelude::*;

impl ChatWidget<'_> {
    /// Push a cell using a synthetic global order key at the bottom of the current request.
    pub(crate) fn history_push(&mut self, cell: impl HistoryCell + 'static) {
        #[cfg(debug_assertions)]
        {
            debug_assert!(
                cell.kind() != HistoryCellType::BackgroundEvent,
                "Background events must use push_background_* helpers"
            );
        }
        let key = self.next_internal_key();
        let _ = self.history_insert_with_key_global_tagged(Box::new(cell), key, "epilogue", None);
    }

    pub(in crate::chatwidget) fn history_insert_plain_state_with_key(
        &mut self,
        state: PlainMessageState,
        key: OrderKey,
        tag: &'static str,
    ) -> usize {
        let cell = crate::history_cell::PlainHistoryCell::from_state(state.clone());
        self.history_insert_with_key_global_tagged(
            Box::new(cell),
            key,
            tag,
            Some(HistoryDomainRecord::Plain(state)),
        )
    }

    pub(crate) fn history_push_plain_state(&mut self, state: PlainMessageState) {
        let key = self.next_internal_key();
        let _ = self.history_insert_plain_state_with_key(state, key, "epilogue");
    }

    pub(in crate::chatwidget) fn history_push_plain_paragraphs<I, S>(
        &mut self,
        kind: PlainMessageKind,
        lines: I,
    ) where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let role = history_cell::plain_role_for_kind(kind);
        let state = history_cell::plain_message_state_from_paragraphs(kind, role, lines);
        self.history_push_plain_state(state);
    }

    pub(in crate::chatwidget) fn history_push_diff(&mut self, title: Option<String>, diff_output: String) {
        let record = history_cell::diff_record_from_string(
            title.unwrap_or_default(),
            &diff_output,
        );
        let key = self.next_internal_key();
        let _ = self.history_insert_with_key_global_tagged(
            Box::new(history_cell::DiffCell::from_record(record.clone())),
            key,
            "diff",
            Some(HistoryDomainRecord::Diff(record)),
        );
    }
}

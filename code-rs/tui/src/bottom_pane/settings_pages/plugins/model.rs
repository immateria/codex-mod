use super::*;

impl PluginsSettingsView {
    pub(crate) fn new(
        shared_state: Arc<Mutex<PluginsSharedState>>,
        roots: Vec<AbsolutePathBuf>,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut list_state = ScrollState::new();
        list_state.selected_idx = Some(0);

        let mut sources_list_state = ScrollState::new();
        sources_list_state.selected_idx = Some(0);

        let view = Self {
            shared_state,
            roots,
            list_state: Cell::new(list_state),
            list_viewport_rows: Cell::new(DEFAULT_LIST_VIEWPORT_ROWS),
            sources_list_state: Cell::new(sources_list_state),
            sources_list_viewport_rows: Cell::new(DEFAULT_LIST_VIEWPORT_ROWS),
            sources_editor: SourcesEditorState::new(),
            mode: Mode::List,
            hovered_detail_button: None,
            focused_detail_button: DetailAction::Back,
            hovered_confirm_button: None,
            focused_confirm_button: ConfirmAction::Cancel,
            hovered_sources_confirm_button: None,
            focused_sources_confirm_button: SourcesConfirmRemoveAction::Cancel,
            app_event_tx,
            is_complete: false,
        };

        let should_request_list = {
            let snapshot = view
                .shared_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match &snapshot.list {
                crate::chatwidget::PluginsListState::Uninitialized => true,
                crate::chatwidget::PluginsListState::Loading { roots, .. }
                | crate::chatwidget::PluginsListState::Ready { roots, .. }
                | crate::chatwidget::PluginsListState::Failed { roots, .. } => *roots != view.roots,
            }
        };
        if should_request_list {
            view.request_plugin_list(/*force_remote_sync*/ false);
        }
        view
    }

    pub(crate) fn framed(&self) -> PluginsSettingsViewFramed<'_> {
        crate::bottom_pane::chrome_view::Framed::new(self)
    }

    pub(crate) fn content_only(&self) -> PluginsSettingsViewContentOnly<'_> {
        crate::bottom_pane::chrome_view::ContentOnly::new(self)
    }

    pub(crate) fn framed_mut(&mut self) -> PluginsSettingsViewFramedMut<'_> {
        crate::bottom_pane::chrome_view::FramedMut::new(self)
    }

    pub(crate) fn content_only_mut(&mut self) -> PluginsSettingsViewContentOnlyMut<'_> {
        crate::bottom_pane::chrome_view::ContentOnlyMut::new(self)
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.is_complete
    }
}

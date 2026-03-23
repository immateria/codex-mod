impl ChatWidget<'_> {
    pub(in crate::chatwidget) fn build_cell_from_record(&self, record: &HistoryRecord) -> Option<Box<dyn HistoryCell>> {
        use crate::history_cell;

        match record {
            HistoryRecord::PlainMessage(state) => Some(Box::new(
                history_cell::PlainHistoryCell::from_state(state.clone()),
            )),
            HistoryRecord::WaitStatus(state) => {
                Some(Box::new(history_cell::WaitStatusCell::from_state(state.clone())))
            }
            HistoryRecord::Loading(state) => {
                Some(Box::new(history_cell::LoadingCell::from_state(state.clone())))
            }
            HistoryRecord::RunningTool(state) => Some(Box::new(
                history_cell::RunningToolCallCell::from_state(state.clone()),
            )),
            HistoryRecord::ToolCall(state) => Some(Box::new(
                history_cell::ToolCallCell::from_state(state.clone()),
            )),
            HistoryRecord::PlanUpdate(state) => Some(Box::new(
                history_cell::PlanUpdateCell::from_state(state.clone()),
            )),
            HistoryRecord::UpgradeNotice(state) => Some(Box::new(
                history_cell::UpgradeNoticeCell::from_state(state.clone()),
            )),
            HistoryRecord::Reasoning(state) => Some(Box::new(
                history_cell::CollapsibleReasoningCell::from_state(state.clone()),
            )),
            HistoryRecord::Exec(state) => {
                Some(Box::new(history_cell::ExecCell::from_record(state.clone())))
            }
            HistoryRecord::MergedExec(state) => Some(Box::new(
                history_cell::MergedExecCell::from_state(state.clone()),
            )),
            HistoryRecord::AssistantStream(state) => Some(Box::new(
                history_cell::StreamingContentCell::from_state(
                    state.clone(),
                    self.config.file_opener,
                    self.config.cwd.clone(),
                ),
            )),
            HistoryRecord::AssistantMessage(state) => Some(Box::new(
                history_cell::AssistantMarkdownCell::from_state(state.clone(), &self.config),
            )),
            HistoryRecord::ProposedPlan(state) => Some(Box::new(
                history_cell::ProposedPlanCell::from_state(state.clone(), &self.config),
            )),
            HistoryRecord::Diff(state) => {
                Some(Box::new(history_cell::DiffCell::from_record(state.clone())))
            }
            HistoryRecord::Patch(state) => {
                Some(Box::new(history_cell::PatchSummaryCell::from_record(state.clone())))
            }
            HistoryRecord::Explore(state) => {
                Some(Box::new(history_cell::ExploreAggregationCell::from_record(state.clone())))
            }
            HistoryRecord::RateLimits(state) => Some(Box::new(
                history_cell::RateLimitsCell::from_record(state.clone()),
            )),
            HistoryRecord::BackgroundEvent(state) => {
                Some(Box::new(history_cell::BackgroundEventCell::new(state.clone())))
            }
            HistoryRecord::Image(state) => {
                let cell = history_cell::ImageOutputCell::from_record(state.clone());
                self.ensure_image_cell_picker(&cell);
                Some(Box::new(cell))
            }
            HistoryRecord::Context(state) => Some(Box::new(
                history_cell::ContextCell::new(state.clone()),
            )),
            HistoryRecord::Notice(state) => Some(Box::new(
                history_cell::PlainHistoryCell::from_notice_record(state.clone()),
            )),
        }
    }
}

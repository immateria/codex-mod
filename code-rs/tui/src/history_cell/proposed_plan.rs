use super::*;

use crate::history::state::ProposedPlanState;
use code_core::config::Config;
use code_core::config_types::UriBasedFileOpener;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

pub(crate) struct ProposedPlanCell {
    state: ProposedPlanState,
    file_opener: UriBasedFileOpener,
    cwd: PathBuf,
    rendered_lines_cache: RefCell<Option<Rc<Vec<Line<'static>>>>>,
}

impl ProposedPlanCell {
    pub(crate) fn from_state(state: ProposedPlanState, cfg: &Config) -> Self {
        Self {
            state,
            file_opener: cfg.file_opener,
            cwd: cfg.cwd.clone(),
            rendered_lines_cache: RefCell::new(None),
        }
    }

    pub(crate) fn state(&self) -> &ProposedPlanState {
        &self.state
    }

    pub(crate) fn markdown(&self) -> &str {
        &self.state.markdown
    }

    fn ensure_rendered_lines(&self) -> Rc<Vec<Line<'static>>> {
        if let Some(lines) = self.rendered_lines_cache.borrow().as_ref() {
            return Rc::clone(lines);
        }

        let mut out: Vec<Line<'static>> = Vec::new();

        let title_style = Style::default()
            .fg(crate::colors::text_bright())
            .add_modifier(Modifier::BOLD);
        out.push(Line::from(Span::styled("Proposed plan", title_style)));
        out.push(Line::from(Span::styled(
            "Extracted from assistant plan output",
            Style::default().fg(crate::colors::text_dim()),
        )));
        out.push(Line::from(""));

        let is_empty = self.state.markdown.trim().is_empty();
        if is_empty {
            out.push(Line::from(Span::styled(
                "(empty)",
                Style::default().fg(crate::colors::text_dim()),
            )));
        } else {
            crate::markdown::append_markdown_with_opener_and_cwd_and_bold(
                &self.state.markdown,
                &mut out,
                self.file_opener,
                &self.cwd,
                false,
            );
        }

        let fg = crate::colors::text_bright();
        for (idx, line) in out.iter_mut().enumerate().skip(3) {
            if is_empty && idx == 3 {
                continue;
            }
            line.style = line.style.patch(Style::default().fg(fg));
        }

        let out = Rc::new(trim_empty_lines(out));
        *self.rendered_lines_cache.borrow_mut() = Some(Rc::clone(&out));
        out
    }
}

impl HistoryCell for ProposedPlanCell {
    fn display_lines(&self) -> Vec<Line<'static>> {
        self.ensure_rendered_lines().as_ref().clone()
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::ProposedPlan
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

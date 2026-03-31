use super::*;
use crate::app_event::AppEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

#[test]
fn auto_review_status_stays_left_with_auto_drive_footer() {
    let (tx, _rx) = std::sync::mpsc::channel::<AppEvent>();
    let app_tx = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(true, app_tx, true);

    composer.auto_drive_active = true;
    composer.standard_terminal_hint = Some("Esc stop\tCtrl+S settings".to_string());
    composer.set_auto_review_status(Some(AutoReviewFooterStatus {
        status: AutoReviewIndicatorStatus::Running,
        findings: None,
        phase: AutoReviewPhase::Reviewing,
    }));

    let area = Rect {
        x: 0,
        y: 0,
        width: 64,
        height: 1,
    };
    let mut buf = Buffer::empty(area);
    composer.render_footer(area, &mut buf);

    let line: String = (0..area.width)
        .map(|x| buf[(area.x + x, area.y)].symbol().to_string())
        .collect();

    let auto_idx = line
        .find("Auto Review")
        .expect("footer should show auto review text");
    let esc_idx = line.find("Esc stop").unwrap_or(line.len());

    assert!(auto_idx < esc_idx, "Auto Review status should be left-most");
}

#[test]
fn footer_shows_1m_context_suffix_when_extended_context_is_active() {
    let (tx, _rx) = std::sync::mpsc::channel::<AppEvent>();
    let app_tx = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(true, app_tx, true);

    let token_usage = TokenUsage {
        input_tokens: 13_290,
        cached_input_tokens: 0,
        output_tokens: 0,
        reasoning_output_tokens: 0,
        total_tokens: 13_290,
    };
    composer.set_token_usage(
        token_usage,
        Some(EXTENDED_CONTEXT_WINDOW_1M),
        Some(ContextMode::OneM),
    );

    let area = Rect {
        x: 0,
        y: 0,
        width: 96,
        height: 1,
    };
    let mut buf = Buffer::empty(area);
    composer.render_footer(area, &mut buf);

    let line: String = (0..area.width)
        .map(|x| buf[(area.x + x, area.y)].symbol().to_string())
        .collect();

    assert!(line.contains("13,290 tokens"));
    assert!(line.contains("1M Context"));
}

#[test]
fn footer_shows_1m_auto_suffix_when_auto_context_is_active() {
    let (tx, _rx) = std::sync::mpsc::channel::<AppEvent>();
    let app_tx = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(true, app_tx, true);

    let token_usage = TokenUsage {
        input_tokens: 13_290,
        cached_input_tokens: 0,
        output_tokens: 0,
        reasoning_output_tokens: 0,
        total_tokens: 13_290,
    };
    composer.set_token_usage(
        token_usage,
        Some(EXTENDED_CONTEXT_WINDOW_1M),
        Some(ContextMode::Auto),
    );

    let area = Rect {
        x: 0,
        y: 0,
        width: 96,
        height: 1,
    };
    let mut buf = Buffer::empty(area);

    composer.render_footer(area, &mut buf);

    let line: String = (0..area.width)
        .map(|x| buf[(area.x + x, area.y)].symbol().to_string())
        .collect();
    assert!(line.contains("1M Auto"));
}

#[test]
fn footer_shows_checking_context_while_auto_context_check_is_running() {
    let (tx, _rx) = std::sync::mpsc::channel::<AppEvent>();
    let app_tx = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(true, app_tx, true);

    let token_usage = TokenUsage {
        input_tokens: 13_290,
        cached_input_tokens: 0,
        output_tokens: 0,
        reasoning_output_tokens: 0,
        total_tokens: 13_290,
    };
    composer.set_token_usage(
        token_usage,
        Some(EXTENDED_CONTEXT_WINDOW_1M),
        Some(ContextMode::Auto),
    );
    composer.set_auto_context_phase(Some(AutoContextPhase::Checking));

    let area = Rect {
        x: 0,
        y: 0,
        width: 96,
        height: 1,
    };
    let mut buf = Buffer::empty(area);

    composer.render_footer(area, &mut buf);

    let line: String = (0..area.width)
        .map(|x| buf[(area.x + x, area.y)].symbol().to_string())
        .collect();
    assert!(line.contains("Checking context..."));
}

#[test]
fn footer_shows_compacting_while_auto_context_compact_is_running() {
    let (tx, _rx) = std::sync::mpsc::channel::<AppEvent>();
    let app_tx = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(true, app_tx, true);

    let token_usage = TokenUsage {
        input_tokens: 13_290,
        cached_input_tokens: 0,
        output_tokens: 0,
        reasoning_output_tokens: 0,
        total_tokens: 13_290,
    };
    composer.set_token_usage(
        token_usage,
        Some(EXTENDED_CONTEXT_WINDOW_1M),
        Some(ContextMode::Auto),
    );
    composer.set_auto_context_phase(Some(AutoContextPhase::Compacting));

    let area = Rect {
        x: 0,
        y: 0,
        width: 96,
        height: 1,
    };
    let mut buf = Buffer::empty(area);

    composer.render_footer(area, &mut buf);

    let line: String = (0..area.width)
        .map(|x| buf[(area.x + x, area.y)].symbol().to_string())
        .collect();
    assert!(line.contains("Compacting..."));
}

#[test]
fn map_status_message_shows_searching_for_search_status() {
    assert_eq!(
        ChatComposer::map_status_message("Search"),
        "Searching".to_string()
    );
    assert_eq!(
        ChatComposer::map_status_message("searching files"),
        "Searching".to_string()
    );
    assert_eq!(
        ChatComposer::map_status_message("waiting for user input"),
        "Working".to_string()
    );
    assert_eq!(
        ChatComposer::map_status_message("chat completions model"),
        "Responding".to_string()
    );
}

#[test]
fn map_status_message_shows_connecting_for_connecting_status() {
    assert_eq!(
        ChatComposer::map_status_message("connecting to model"),
        "Connecting".to_string(),
    );
    assert_eq!(
        ChatComposer::map_status_message("(connecting to model)"),
        "Connecting".to_string(),
    );
}

#[test]
fn subagent_popup_prefill_does_not_record_submission_history() {
    let (tx, _rx) = std::sync::mpsc::channel::<AppEvent>();
    let app_tx = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(true, app_tx, true);
    composer.set_subagent_commands(vec!["qwertyagent".to_string()]);
    composer.textarea.set_text("/qwe");
    composer.sync_command_popup();

    let (result, handled) = composer.confirm_slash_popup_selection();

    assert_eq!(result, InputResult::None);
    assert!(handled);
    assert_eq!(composer.textarea.text(), "/qwertyagent ");
    composer.textarea.set_text("");
    assert!(!composer.try_history_up());
}

#[test]
fn footer_only_mode_uses_footer_height_and_hides_cursor() {
    let (tx, _rx) = std::sync::mpsc::channel::<AppEvent>();
    let app_tx = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(true, app_tx, true);
    composer.set_render_mode(ComposerRenderMode::FooterOnly);
    composer.standard_terminal_hint = Some("Terminal mode".to_string());
    composer.active_popup = ActivePopup::Command(CommandPopup::new_with_filter(true));

    assert_eq!(composer.footer_height(), 1);
    assert_eq!(composer.desired_height(80), 1);
    assert_eq!(
        composer.cursor_pos(Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 3,
        }),
        None
    );
}

#[test]
fn insert_selected_path_quotes_and_escapes_internal_quotes() {
    let (tx, _rx) = std::sync::mpsc::channel::<AppEvent>();
    let app_tx = AppEventSender::new(tx);
    let mut composer = ChatComposer::new(true, app_tx, true);
    composer.textarea.set_text("@fi");
    composer.textarea.set_cursor(3);

    composer.insert_selected_path("/tmp/my \"quoted\" file.txt");

    assert_eq!(
        composer.textarea.text(),
        "\"/tmp/my \\\"quoted\\\" file.txt\" "
    );
}

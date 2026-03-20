            match event {
                AppEvent::KeyEvent(mut key_event) => {
                    if self.timing_enabled { self.timing.on_key(); }
                    #[cfg(windows)]
                    {
                        use crossterm::event::KeyCode;
                        use crossterm::event::KeyEventKind;
                        if matches!(key_event.kind, KeyEventKind::Repeat) {
                            match key_event.code {
                                KeyCode::Left
                                | KeyCode::Right
                                | KeyCode::Up
                                | KeyCode::Down
                                | KeyCode::Home
                                | KeyCode::End
                                | KeyCode::Backspace
                                | KeyCode::Delete => {}
                                _ => continue,
                            }
                        }
                    }
                    // On terminals without keyboard enhancement flags (notably some Windows
                    // Git Bash/mintty setups), crossterm may emit duplicate key-up events or
                    // only report releases. Track which keys were seen as pressed so matching
                    // releases can be dropped, and synthesize a press when a release arrives
                    // without a prior press.
                    if !self.enhanced_keys_supported {
                        let key_code = key_event.code;
                        match key_event.kind {
                            KeyEventKind::Press | KeyEventKind::Repeat => {
                                self.non_enhanced_pressed_keys.insert(key_code);
                            }
                            KeyEventKind::Release => {
                                if self.non_enhanced_pressed_keys.remove(&key_code) {
                                    continue;
                                }

                                let mut release_handled = false;
                                if let KeyCode::Char(ch) = key_code {
                                    let alts: Vec<char> = ch
                                        .to_lowercase()
                                        .chain(ch.to_uppercase())
                                        .filter(|&c| c != ch)
                                        .collect();

                                    for alt in alts {
                                        if self
                                            .non_enhanced_pressed_keys
                                            .remove(&KeyCode::Char(alt))
                                        {
                                            release_handled = true;
                                            break;
                                        }
                                    }
                                }

                                if release_handled {
                                    continue;
                                }

                                key_event = KeyEvent::new(
                                    Self::normalize_non_enhanced_release_code(key_event.code),
                                    key_event.modifiers,
                                );
                            }
                        }
                    }
                    // Reset double‑Esc timer on any non‑Esc key
                    if !matches!(key_event.code, KeyCode::Esc) {
                        self.last_esc_time = None;
                    }

                    match key_event {
                        KeyEvent { code: KeyCode::Esc, kind: KeyEventKind::Press | KeyEventKind::Repeat, .. } => {
                            if let AppState::Chat { widget } = &mut self.app_state
                                && widget.handle_app_esc(key_event, &mut self.last_esc_time) {
                                    continue;
                                }
                            // Otherwise fall through
                        }
                        // Explicit image paste shortcut fallback.
                        //
                        // Standard text paste shortcuts (Ctrl/Cmd+V, Ctrl+Shift+V,
                        // Shift+Insert) must flow through terminal paste events to avoid
                        // truncating text on terminals that also emit partial key streams.
                        // Keep an opt-in image path on Ctrl+Alt+V for environments that
                        // don't emit Event::Paste for image clipboards.
                        key_event if is_image_clipboard_paste_shortcut(&key_event) => {
                            self.dispatch_paste_event(String::new());
                        }
                        KeyEvent {
                            code: KeyCode::Char('m'),
                            modifiers: crossterm::event::KeyModifiers::CONTROL,
                            kind: KeyEventKind::Press,
                            ..
                        } => {
                            // Toggle mouse capture to allow text selection
                            use crossterm::event::DisableMouseCapture;
                            use crossterm::event::EnableMouseCapture;
                            use crossterm::execute;
                            use std::io::stdout;

                            // Static variable to track mouse capture state
                            static mut MOUSE_CAPTURE_ENABLED: bool = true;

                            unsafe {
                                MOUSE_CAPTURE_ENABLED = !MOUSE_CAPTURE_ENABLED;
                                if MOUSE_CAPTURE_ENABLED {
                                    let _ = execute!(stdout(), EnableMouseCapture);
                                } else {
                                    let _ = execute!(stdout(), DisableMouseCapture);
                                }
                            }
                            self.app_event_tx.send(AppEvent::RequestRedraw);
                        }
                        KeyEvent {
                            code: KeyCode::Char('c'),
                            modifiers: crossterm::event::KeyModifiers::CONTROL,
                            kind: KeyEventKind::Press,
                            ..
                        } => match &mut self.app_state {
                            AppState::Chat { widget } => {
                                match widget.on_ctrl_c() {
                                    crate::bottom_pane::CancellationEvent::Handled => {
                                        if widget.ctrl_c_requests_exit() {
                                            self.app_event_tx.send(AppEvent::ExitRequest);
                                        }
                                    }
                                    crate::bottom_pane::CancellationEvent::Ignored => {}
                                }
                            }
                            AppState::Onboarding { .. } => { self.app_event_tx.send(AppEvent::ExitRequest); }
                        },
                        KeyEvent {
                            code: KeyCode::Char('z'),
                            modifiers: crossterm::event::KeyModifiers::CONTROL,
                            kind: KeyEventKind::Press,
                            ..
                        } => {
                            // Prefer in-app undo in Chat (composer) over shell suspend.
                            match &mut self.app_state {
                                AppState::Chat { widget } => {
                                    widget.handle_key_event(key_event);
                                    self.app_event_tx.send(AppEvent::RequestRedraw);
                                }
                                AppState::Onboarding { .. } => {
                                    #[cfg(unix)]
                                    {
                                        self.suspend(terminal)?;
                                    }
                                    // No-op on non-Unix platforms.
                                }
                            }
                        }
                        KeyEvent {
                            code: KeyCode::Char('r'),
                            modifiers: crossterm::event::KeyModifiers::CONTROL,
                            kind: KeyEventKind::Press,
                            ..
                        }
                        | KeyEvent {
                            code: KeyCode::Char('r'),
                            modifiers: crossterm::event::KeyModifiers::CONTROL,
                            kind: KeyEventKind::Repeat,
                            ..
                        } => {
                            // Toggle reasoning/thinking visibility (Ctrl+R)
                            match &mut self.app_state {
                                AppState::Chat { widget } => {
                                    widget.toggle_reasoning_visibility();
                                }
                                AppState::Onboarding { .. } => {}
                            }
                        }
                        KeyEvent {
                            code: KeyCode::Char('c'),
                            modifiers,
                            kind: KeyEventKind::Press,
                            ..
                        } if modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
                            && modifiers.contains(crossterm::event::KeyModifiers::SHIFT) =>
                        {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.toggle_context_expansion();
                            }
                        }
                        KeyEvent {
                            code: KeyCode::Char('t'),
                            modifiers: crossterm::event::KeyModifiers::CONTROL,
                            kind: KeyEventKind::Press | KeyEventKind::Repeat,
                            ..
                        } => {
                            let _ = self.toggle_screen_mode(terminal);
                            // Propagate mode to widget so it can adapt layout
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.set_standard_terminal_mode(!self.alt_screen_active);
                            }
                        }
                        KeyEvent {
                            code: KeyCode::Char('d'),
                            modifiers: crossterm::event::KeyModifiers::CONTROL,
                            kind: KeyEventKind::Press,
                            ..
                        } => {
                            // Toggle diffs overlay
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.toggle_diffs_popup();
                            }
                        }
                        // (Ctrl+Y disabled): Previously cycled syntax themes; now intentionally no-op
                        KeyEvent {
                            kind: KeyEventKind::Press | KeyEventKind::Repeat,
                            ..
                        } => {
                            self.dispatch_key_event(key_event);
                        }
                        _ => {
                            // Ignore Release key events.
                        }
                    };
                }
                AppEvent::MouseEvent(mouse_event) => {
                    self.dispatch_mouse_event(mouse_event);
                }
                AppEvent::Paste(text) => {
                    self.dispatch_paste_event(text);
                }
                AppEvent::OpenExternalEditor { initial } => {
                    let was_alt_screen = self.alt_screen_active;
                    self.input_suspended.store(true, Ordering::Release);
                    let editor_result = match tui::restore() {
                        Ok(()) => external_editor::run_editor(&initial),
                        Err(err) => Err(external_editor::ExternalEditorError::LaunchFailed(format!(
                            "Failed to reset terminal: {err}",
                        ))),
                    };
                    let (new_terminal, new_terminal_info) = tui::init(&self.config)?;
                    *terminal = new_terminal;
                    self.terminal_info = new_terminal_info;
                    terminal.clear()?;
                    if was_alt_screen {
                        self.alt_screen_active = true;
                    } else {
                        let _ = tui::leave_alt_screen_only();
                        self.alt_screen_active = false;
                    }
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_standard_terminal_mode(!self.alt_screen_active);
                    }
                    self.input_suspended.store(false, Ordering::Release);
                    if let AppState::Chat { widget } = &mut self.app_state {
                        match editor_result {
                            Ok(text) => widget.set_composer_text(text),
                            Err(err) => widget.debug_notice(err.to_string()),
                        }
                    }
                    self.app_event_tx.send(AppEvent::RequestRedraw);
                }
                AppEvent::RegisterPastedImage { placeholder, path } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.register_pasted_image(placeholder, path);
                    }
                }
                AppEvent::CodexEvent(event) => {
                    self.dispatch_code_event(*event);
                }
                AppEvent::ExitRequest => {
                    // Stop background threads and break the UI loop.
                    self.commit_anim_running.store(false, Ordering::Release);
                    self.input_running.store(false, Ordering::Release);
                    break 'main;
                }
                event => {
                    include!("terminal.rs");
                }
            }

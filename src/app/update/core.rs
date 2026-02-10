use super::super::messages::Message;
use super::super::state::{App, TEXT_SCROLL_ID};
use super::Effect;
use crate::cache::{load_bookmark, load_epub_config, remember_source_path};
use crate::calibre::{CalibreBook, CalibreColumn};
use crate::config::load_config;
use crate::epub_loader::load_book_content;
use iced::Event;
use iced::event;
use iced::keyboard::{self, Key, Modifiers, key};
use iced::time;
use iced::window;
use iced::{Subscription, Task};
use std::cmp::Ordering;
use std::path::Path;
use std::time::Duration;
use tracing::{info, warn};

impl App {
    pub fn subscription(app: &App) -> Subscription<Message> {
        let mut subscriptions: Vec<Subscription<Message>> = vec![
            iced::window::resize_events().map(|(_id, size)| Message::WindowResized {
                width: size.width,
                height: size.height,
            }),
            event::listen_with(runtime_event_to_message),
        ];

        if app.tts.running {
            subscriptions.push(time::every(Duration::from_millis(50)).map(Message::Tick));
        }

        Subscription::batch(subscriptions)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let effects = self.reduce(message);
        if effects.is_empty() {
            Task::none()
        } else {
            Task::batch(effects.into_iter().map(|effect| self.run_effect(effect)))
        }
    }

    fn reduce(&mut self, message: Message) -> Vec<Effect> {
        let mut effects = Vec::new();

        match message {
            Message::NextPage => self.handle_next_page(&mut effects),
            Message::PreviousPage => self.handle_previous_page(&mut effects),
            Message::FontSizeChanged(size) => self.handle_font_size_changed(size, &mut effects),
            Message::ToggleTheme => self.handle_toggle_theme(&mut effects),
            Message::ToggleSettings => self.handle_toggle_settings(&mut effects),
            Message::ToggleSearch => self.handle_toggle_search(&mut effects),
            Message::SearchQueryChanged(query) => self.handle_search_query_changed(query),
            Message::SearchSubmit => self.handle_search_submit(&mut effects),
            Message::SearchNext => self.handle_search_next(&mut effects),
            Message::SearchPrev => self.handle_search_prev(&mut effects),
            Message::ToggleRecentBooks => self.handle_toggle_recent_books(),
            Message::OpenRecentBook(path) => self.handle_open_recent_book(path, &mut effects),
            Message::ToggleCalibreBrowser => self.handle_toggle_calibre_browser(&mut effects),
            Message::OpenPathInputChanged(path) => self.handle_open_path_input_changed(path),
            Message::OpenPathRequested => self.handle_open_path_requested(&mut effects),
            Message::RefreshCalibreBooks => self.handle_refresh_calibre_books(&mut effects),
            Message::CalibreSearchQueryChanged(query) => {
                self.handle_calibre_search_query_changed(query)
            }
            Message::SortCalibreBy(column) => self.handle_sort_calibre_by(column),
            Message::CalibreBooksLoaded { books, error } => {
                self.handle_calibre_books_loaded(books, error)
            }
            Message::OpenCalibreBook(book_id) => {
                self.handle_open_calibre_book(book_id, &mut effects)
            }
            Message::CalibreBookResolved {
                book_id,
                path,
                error,
            } => self.handle_calibre_book_resolved(book_id, path, error, &mut effects),
            Message::BookLoaded {
                path,
                book,
                config,
                bookmark,
            } => self.handle_book_loaded(path, book, config, bookmark, &mut effects),
            Message::BookLoadFailed { path, error } => self.handle_book_load_failed(path, error),
            Message::ToggleTextOnly => self.handle_toggle_text_only(&mut effects),
            Message::FontFamilyChanged(family) => {
                self.handle_font_family_changed(family, &mut effects);
            }
            Message::FontWeightChanged(weight) => {
                self.handle_font_weight_changed(weight, &mut effects);
            }
            Message::LineSpacingChanged(spacing) => {
                self.handle_line_spacing_changed(spacing, &mut effects);
            }
            Message::MarginHorizontalChanged(margin) => {
                self.handle_margin_horizontal_changed(margin, &mut effects);
            }
            Message::MarginVerticalChanged(margin) => {
                self.handle_margin_vertical_changed(margin, &mut effects);
            }
            Message::WordSpacingChanged(spacing) => {
                self.handle_word_spacing_changed(spacing, &mut effects);
            }
            Message::LetterSpacingChanged(spacing) => {
                self.handle_letter_spacing_changed(spacing, &mut effects);
            }
            Message::LinesPerPageChanged(lines) => {
                self.handle_lines_per_page_changed(lines, &mut effects);
            }
            Message::DayHighlightChanged(component, value) => {
                self.handle_day_highlight_changed(component, value, &mut effects);
            }
            Message::PauseAfterSentenceChanged(pause) => {
                self.handle_pause_after_sentence_changed(pause, &mut effects);
            }
            Message::NightHighlightChanged(component, value) => {
                self.handle_night_highlight_changed(component, value, &mut effects);
            }
            Message::AutoScrollTtsChanged(enabled) => {
                self.handle_auto_scroll_tts_changed(enabled, &mut effects);
            }
            Message::CenterSpokenSentenceChanged(centered) => {
                self.handle_center_spoken_sentence_changed(centered, &mut effects);
            }
            Message::ToggleTtsControls => self.handle_toggle_tts_controls(&mut effects),
            Message::JumpToCurrentAudio => self.handle_jump_to_current_audio(&mut effects),
            Message::TogglePlayPause => self.handle_toggle_play_pause(&mut effects),
            Message::RepeatCurrentSentence => self.handle_repeat_current_sentence(&mut effects),
            Message::SafeQuit => effects.push(Effect::QuitSafely),
            Message::Play => self.handle_play(&mut effects),
            Message::PlayFromPageStart => self.handle_play_from_page_start(&mut effects),
            Message::PlayFromCursor(idx) => self.handle_play_from_cursor(idx, &mut effects),
            Message::Pause => self.handle_pause(&mut effects),
            Message::SetTtsSpeed(speed) => self.handle_set_tts_speed(speed, &mut effects),
            Message::SetTtsVolume(volume) => self.handle_set_tts_volume(volume, &mut effects),
            Message::SeekForward => self.handle_seek_forward(&mut effects),
            Message::SeekBackward => self.handle_seek_backward(&mut effects),
            Message::SentenceClicked(idx) => self.handle_sentence_clicked(idx, &mut effects),
            Message::WindowResized { width, height } => {
                self.handle_window_resized(width, height, &mut effects);
            }
            Message::WindowMoved { x, y } => {
                self.handle_window_moved(x, y, &mut effects);
            }
            Message::KeyPressed { key, modifiers } => {
                if let Some(shortcut) = self.shortcut_message_for_key(key, modifiers) {
                    effects.extend(self.reduce(shortcut));
                }
            }
            Message::Scrolled {
                offset,
                viewport_width,
                viewport_height,
                content_width,
                content_height,
            } => self.handle_scrolled(
                offset,
                viewport_width,
                viewport_height,
                content_width,
                content_height,
                &mut effects,
            ),
            Message::TtsPrepared {
                page,
                start_idx,
                request_id,
                files,
            } => self.handle_tts_prepared(page, start_idx, request_id, files, &mut effects),
            Message::Tick(now) => self.handle_tick(now, &mut effects),
        }

        if self.text_only_mode {
            self.ensure_text_only_preview_for_page(self.reader.current_page);
        }
        self.update_search_matches();

        effects
    }

    fn run_effect(&mut self, effect: Effect) -> Task<Message> {
        match effect {
            Effect::SaveConfig => {
                self.save_epub_config();
                Task::none()
            }
            Effect::SaveBookmark => {
                self.persist_bookmark();
                Task::none()
            }
            Effect::StartTts { page, sentence_idx } => self.start_playback_from(page, sentence_idx),
            Effect::StopTts => {
                self.stop_playback();
                Task::none()
            }
            Effect::ScrollTo(offset) => {
                self.bookmark.last_scroll_offset = offset;
                iced::widget::scrollable::snap_to(TEXT_SCROLL_ID.clone(), offset)
            }
            Effect::AutoScrollToCurrent => {
                if !self.config.auto_scroll_tts {
                    return Task::none();
                }
                if let Some(idx) = self.tts.current_sentence_idx {
                    if let Some(offset) = self.scroll_offset_for_sentence(idx) {
                        self.bookmark.last_scroll_offset = offset;
                        return iced::widget::scrollable::snap_to(TEXT_SCROLL_ID.clone(), offset);
                    }
                }
                Task::none()
            }
            Effect::LoadCalibreBooks { force_refresh } => {
                self.calibre.loading = true;
                self.calibre.error = None;
                let config = self.calibre.config.clone();
                Task::perform(
                    async move {
                        match crate::calibre::load_books(&config, force_refresh) {
                            Ok(books) => Message::CalibreBooksLoaded { books, error: None },
                            Err(err) => Message::CalibreBooksLoaded {
                                books: Vec::new(),
                                error: Some(err.to_string()),
                            },
                        }
                    },
                    |message| message,
                )
            }
            Effect::ResolveCalibreBook { book, config } => Task::perform(
                async move {
                    match crate::calibre::materialize_book_path(&config, &book) {
                        Ok(path) => Message::CalibreBookResolved {
                            book_id: book.id,
                            path: Some(path),
                            error: None,
                        },
                        Err(err) => Message::CalibreBookResolved {
                            book_id: book.id,
                            path: None,
                            error: Some(err.to_string()),
                        },
                    }
                },
                |message| message,
            ),
            Effect::LoadBook(path) => {
                self.book_loading = true;
                self.book_loading_error = None;
                let requested_path = path.clone();
                Task::perform(
                    async move {
                        let base_config = load_config(Path::new("conf/config.toml"));
                        remember_source_path(&requested_path);
                        let mut config = base_config.clone();
                        if let Some(mut overrides) = load_epub_config(&requested_path) {
                            overrides.log_level = base_config.log_level;
                            overrides.tts_threads = base_config.tts_threads;
                            overrides.tts_progress_log_interval_secs =
                                base_config.tts_progress_log_interval_secs;
                            overrides.key_toggle_play_pause =
                                base_config.key_toggle_play_pause.clone();
                            overrides.key_safe_quit = base_config.key_safe_quit.clone();
                            overrides.key_next_sentence = base_config.key_next_sentence.clone();
                            overrides.key_prev_sentence = base_config.key_prev_sentence.clone();
                            overrides.key_repeat_sentence = base_config.key_repeat_sentence.clone();
                            overrides.key_toggle_search = base_config.key_toggle_search.clone();
                            config = overrides;
                        }
                        let bookmark = load_bookmark(&requested_path);
                        match load_book_content(&requested_path) {
                            Ok(book) => Message::BookLoaded {
                                path: requested_path,
                                book,
                                config,
                                bookmark,
                            },
                            Err(err) => Message::BookLoadFailed {
                                path: requested_path,
                                error: err.to_string(),
                            },
                        }
                    },
                    |message| message,
                )
            }
            Effect::QuitSafely => {
                self.save_epub_config();
                self.persist_bookmark();
                self.stop_playback();
                iced::exit()
            }
        }
    }

    fn shortcut_message_for_key(&self, key: Key, modifiers: Modifiers) -> Option<Message> {
        let pressed = match key.as_ref() {
            Key::Named(key::Named::Space) => "space".to_string(),
            Key::Character(ch) => ch.to_ascii_lowercase(),
            _ => return None,
        };

        if Self::shortcut_matches(
            &self.config.key_toggle_play_pause,
            "space",
            &pressed,
            modifiers,
        ) {
            Some(Message::TogglePlayPause)
        } else if Self::shortcut_matches(&self.config.key_safe_quit, "q", &pressed, modifiers) {
            Some(Message::SafeQuit)
        } else if Self::shortcut_matches(&self.config.key_next_sentence, "f", &pressed, modifiers) {
            Some(Message::SeekForward)
        } else if Self::shortcut_matches(&self.config.key_prev_sentence, "s", &pressed, modifiers) {
            Some(Message::SeekBackward)
        } else if Self::shortcut_matches(&self.config.key_repeat_sentence, "r", &pressed, modifiers)
        {
            Some(Message::RepeatCurrentSentence)
        } else if Self::shortcut_matches(
            &self.config.key_toggle_search,
            "ctrl+f",
            &pressed,
            modifiers,
        ) {
            Some(Message::ToggleSearch)
        } else {
            None
        }
    }

    fn shortcut_matches(raw: &str, fallback: &str, pressed: &str, modifiers: Modifiers) -> bool {
        let normalized = Self::normalize_shortcut_token(raw, fallback);

        let mut required_ctrl = false;
        let mut required_alt = false;
        let mut required_logo = false;
        let mut required_shift = false;
        let mut required_key: Option<&str> = None;

        for token in normalized
            .split('+')
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            match token {
                "ctrl" | "control" => required_ctrl = true,
                "alt" => required_alt = true,
                "logo" | "meta" | "super" | "cmd" | "command" => required_logo = true,
                "shift" => required_shift = true,
                key => required_key = Some(key),
            }
        }

        let required_key = required_key.unwrap_or(fallback);
        if pressed != required_key {
            return false;
        }

        modifiers.control() == required_ctrl
            && modifiers.alt() == required_alt
            && modifiers.logo() == required_logo
            && modifiers.shift() == required_shift
    }

    fn normalize_shortcut_token(raw: &str, fallback: &str) -> String {
        let normalized = raw.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            fallback.to_string()
        } else {
            normalized.replace("spacebar", "space")
        }
    }

    fn handle_toggle_search(&mut self, effects: &mut Vec<Effect>) {
        self.search.visible = !self.search.visible;
        if self.search.visible {
            self.update_search_matches();
        } else {
            self.search.error = None;
            self.search.matches.clear();
            self.search.selected_match = 0;
        }
        effects.push(Effect::SaveBookmark);
    }

    fn handle_search_query_changed(&mut self, query: String) {
        self.search.query = query;
        self.update_search_matches();
    }

    fn handle_search_submit(&mut self, effects: &mut Vec<Effect>) {
        self.jump_to_selected_search_match(effects);
    }

    fn handle_search_next(&mut self, effects: &mut Vec<Effect>) {
        if self.search.matches.is_empty() {
            return;
        }
        self.search.selected_match = (self.search.selected_match + 1) % self.search.matches.len();
        self.jump_to_selected_search_match(effects);
    }

    fn handle_search_prev(&mut self, effects: &mut Vec<Effect>) {
        if self.search.matches.is_empty() {
            return;
        }
        if self.search.selected_match == 0 {
            self.search.selected_match = self.search.matches.len().saturating_sub(1);
        } else {
            self.search.selected_match -= 1;
        }
        self.jump_to_selected_search_match(effects);
    }

    fn jump_to_selected_search_match(&mut self, effects: &mut Vec<Effect>) {
        let Some(sentence_idx) = self.selected_search_sentence_idx() else {
            return;
        };
        let Some(display_idx) = self.display_idx_for_search_sentence_idx(sentence_idx) else {
            return;
        };
        self.tts.current_sentence_idx = Some(display_idx);
        effects.push(Effect::AutoScrollToCurrent);
        effects.push(Effect::SaveBookmark);
    }

    fn handle_toggle_recent_books(&mut self) {
        if !self.starter_mode {
            return;
        }
        self.recent.visible = !self.recent.visible;
        if self.recent.visible {
            self.refresh_recent_books();
        }
    }

    fn handle_open_recent_book(&mut self, path: std::path::PathBuf, effects: &mut Vec<Effect>) {
        effects.push(Effect::LoadBook(path));
    }

    fn handle_open_path_input_changed(&mut self, path: String) {
        self.open_path_input = path;
    }

    fn handle_open_path_requested(&mut self, effects: &mut Vec<Effect>) {
        let candidate = std::path::PathBuf::from(self.open_path_input.trim());
        if candidate.as_os_str().is_empty() {
            return;
        }
        if candidate.exists() {
            effects.push(Effect::LoadBook(candidate));
        }
    }

    fn handle_toggle_calibre_browser(&mut self, effects: &mut Vec<Effect>) {
        if !self.starter_mode {
            return;
        }
        self.calibre.visible = !self.calibre.visible;
        if self.calibre.visible && self.calibre.books.is_empty() && !self.calibre.loading {
            effects.push(Effect::LoadCalibreBooks {
                force_refresh: false,
            });
        }
    }

    fn handle_refresh_calibre_books(&mut self, effects: &mut Vec<Effect>) {
        effects.push(Effect::LoadCalibreBooks {
            force_refresh: true,
        });
    }

    fn handle_calibre_search_query_changed(&mut self, query: String) {
        self.calibre.search_query = query;
    }

    fn handle_calibre_books_loaded(
        &mut self,
        books: Vec<crate::calibre::CalibreBook>,
        error: Option<String>,
    ) {
        self.calibre.loading = false;
        self.calibre.error = error;
        self.calibre.books = books;
        self.sort_calibre_books();
    }

    fn handle_open_calibre_book(&mut self, book_id: u64, effects: &mut Vec<Effect>) {
        let Some(book) = self.calibre.books.iter().find(|b| b.id == book_id).cloned() else {
            self.calibre.error = Some(format!("Book id {book_id} not found in loaded catalogue"));
            return;
        };

        if let Some(path) = book.path.clone().filter(|path| path.exists()) {
            effects.push(Effect::LoadBook(path));
        } else {
            self.calibre.error = None;
            effects.push(Effect::ResolveCalibreBook {
                book,
                config: self.calibre.config.clone(),
            });
        }
    }

    fn handle_calibre_book_resolved(
        &mut self,
        book_id: u64,
        path: Option<std::path::PathBuf>,
        error: Option<String>,
        effects: &mut Vec<Effect>,
    ) {
        if let Some(err) = error {
            self.calibre.error = Some(format!("Failed to open book {book_id}: {err}"));
            return;
        }

        let Some(path) = path else {
            self.calibre.error = Some(format!("Book {book_id} could not be resolved"));
            return;
        };

        if let Some(entry) = self.calibre.books.iter_mut().find(|b| b.id == book_id) {
            entry.path = Some(path.clone());
        }
        self.calibre.error = None;
        effects.push(Effect::LoadBook(path));
    }

    fn handle_book_loaded(
        &mut self,
        path: std::path::PathBuf,
        book: crate::epub_loader::LoadedBook,
        config: crate::config::AppConfig,
        bookmark: Option<crate::cache::Bookmark>,
        effects: &mut Vec<Effect>,
    ) {
        let initial_scroll = self.apply_loaded_book(book, config, path.clone(), bookmark);
        self.refresh_recent_books();
        if let Some(offset) = initial_scroll {
            effects.push(Effect::ScrollTo(offset));
        } else if self.tts.current_sentence_idx.is_some() {
            effects.push(Effect::AutoScrollToCurrent);
        }
        info!(path = %path.display(), "Book loaded in-process");
    }

    fn handle_book_load_failed(&mut self, path: std::path::PathBuf, error: String) {
        self.book_loading = false;
        self.book_loading_error = Some(format!("Failed to open {}: {}", path.display(), error));
        warn!(path = %path.display(), "Failed to load book in-process: {error}");
    }

    fn handle_sort_calibre_by(&mut self, column: CalibreColumn) {
        if self.calibre.sort_column == column {
            self.calibre.sort_desc = !self.calibre.sort_desc;
        } else {
            self.calibre.sort_column = column;
            self.calibre.sort_desc = false;
        }
        self.sort_calibre_books();
    }

    fn sort_calibre_books(&mut self) {
        let column = self.calibre.sort_column;
        let desc = self.calibre.sort_desc;
        self.calibre.books.sort_by(|a, b| {
            let mut ord = compare_calibre_books(a, b, column);
            if desc {
                ord = ord.reverse();
            }
            ord
        });
    }
}

fn compare_calibre_books(a: &CalibreBook, b: &CalibreBook, column: CalibreColumn) -> Ordering {
    let primary = match column {
        CalibreColumn::Title => a
            .title
            .to_ascii_lowercase()
            .cmp(&b.title.to_ascii_lowercase()),
        CalibreColumn::Extension => a
            .extension
            .to_ascii_lowercase()
            .cmp(&b.extension.to_ascii_lowercase()),
        CalibreColumn::Author => a
            .authors
            .to_ascii_lowercase()
            .cmp(&b.authors.to_ascii_lowercase()),
        CalibreColumn::Year => a.year.cmp(&b.year),
        CalibreColumn::Size => a.file_size_bytes.cmp(&b.file_size_bytes),
    };

    primary
        .then_with(|| {
            a.title
                .to_ascii_lowercase()
                .cmp(&b.title.to_ascii_lowercase())
        })
        .then_with(|| a.id.cmp(&b.id))
}

fn runtime_event_to_message(
    event: Event,
    status: event::Status,
    _window_id: window::Id,
) -> Option<Message> {
    if status == event::Status::Captured {
        return None;
    }
    match event {
        Event::Window(iced::window::Event::Moved(position)) => Some(Message::WindowMoved {
            x: position.x,
            y: position.y,
        }),
        Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
            Some(Message::KeyPressed { key, modifiers })
        }
        _ => None,
    }
}

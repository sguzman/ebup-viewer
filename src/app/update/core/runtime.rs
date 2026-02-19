use super::super::super::messages::Message;
use super::super::super::state::{App, PendingAppendBatch, TEXT_SCROLL_ID, TtsLifecycle};
use super::super::Effect;
use crate::cache::{load_bookmark, load_epub_config, remember_source_path};
use crate::config::load_config;
use crate::epub_loader::load_book_content;
use iced::Event;
use iced::Task;
use iced::event;
use iced::keyboard;
use iced::mouse;
use iced::window;
use std::path::Path;
use std::time::Duration;
use tracing::info;

impl App {
    pub(super) fn run_effect(&mut self, effect: Effect) -> Task<Message> {
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
            Effect::PrepareTtsBatches {
                page,
                request_id,
                audio_start_idx,
                audio_sentences,
            } => {
                let Some(engine) = self.tts.engine.clone() else {
                    self.tts.lifecycle = TtsLifecycle::Idle;
                    self.tts.pending_append = false;
                    return Task::none();
                };
                let cache_root = crate::cache::tts_dir(&self.epub_path);
                let threads = self.config.tts_threads.max(1);
                let progress_log_interval =
                    Duration::from_secs_f32(self.config.tts_progress_log_interval_secs);
                let clamped_start_idx =
                    audio_start_idx.min(audio_sentences.len().saturating_sub(1));
                let start_idx = if audio_sentences.is_empty() {
                    0
                } else {
                    clamped_start_idx
                };
                let remaining = audio_sentences.len().saturating_sub(start_idx);
                let initial_count = remaining.min(1);
                let initial_sentences = audio_sentences
                    .iter()
                    .skip(start_idx)
                    .take(initial_count)
                    .cloned()
                    .collect::<Vec<_>>();
                let append_sentences = audio_sentences
                    .iter()
                    .skip(start_idx + initial_count)
                    .cloned()
                    .collect::<Vec<_>>();
                self.tts.pending_append = !append_sentences.is_empty();
                self.tts.pending_append_batch = if append_sentences.is_empty() {
                    None
                } else {
                    Some(PendingAppendBatch {
                        page,
                        request_id,
                        start_idx: start_idx + initial_count,
                        audio_sentences: append_sentences,
                    })
                };
                info!(
                    page = page + 1,
                    audio_start_idx = start_idx,
                    initial_count,
                    append_count = self
                        .tts
                        .pending_append_batch
                        .as_ref()
                        .map(|p| p.audio_sentences.len())
                        .unwrap_or(0),
                    request_id,
                    "Split TTS generation into initial playback batch and background append batch"
                );
                let initial_engine = engine.clone();
                let initial_cache = cache_root.clone();
                let initial_task = Task::perform(
                    async move {
                        initial_engine
                            .prepare_batch(
                                initial_cache,
                                initial_sentences,
                                0,
                                threads,
                                progress_log_interval,
                            )
                            .map(|files| Message::TtsPrepared {
                                page,
                                start_idx,
                                request_id,
                                files,
                            })
                            .unwrap_or_else(|_| Message::TtsPrepared {
                                page,
                                start_idx,
                                request_id,
                                files: Vec::new(),
                            })
                    },
                    |msg| msg,
                );
                initial_task
            }
            Effect::PrepareTtsAppend {
                page,
                request_id,
                start_idx,
                audio_sentences,
            } => {
                let Some(engine) = self.tts.engine.clone() else {
                    self.tts.pending_append = false;
                    self.tts.pending_append_batch = None;
                    return Task::none();
                };
                let cache_root = crate::cache::tts_dir(&self.epub_path);
                let threads = self.config.tts_threads.max(1);
                let progress_log_interval =
                    Duration::from_secs_f32(self.config.tts_progress_log_interval_secs);
                Task::perform(
                    async move {
                        engine
                            .prepare_batch(
                                cache_root,
                                audio_sentences,
                                0,
                                threads,
                                progress_log_interval,
                            )
                            .map(|files| Message::TtsAppendPrepared {
                                page,
                                start_idx,
                                request_id,
                                files,
                            })
                            .unwrap_or_else(|_| Message::TtsAppendPrepared {
                                page,
                                start_idx,
                                request_id,
                                files: Vec::new(),
                            })
                    },
                    |msg| msg,
                )
            }
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
                    self.bookmark.pending_sentence_snap = Some(idx);
                    if self.pending_window_resize || self.bookmark.defer_sentence_snap_until_scroll
                    {
                        return Task::none();
                    }
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
                info!(
                    force_refresh,
                    enabled = config.enabled,
                    "Dispatching calibre load task"
                );
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
            Effect::ReadClipboard => iced::clipboard::read().map(Message::ClipboardRead),
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
                            overrides.key_toggle_settings = base_config.key_toggle_settings.clone();
                            overrides.key_toggle_stats = base_config.key_toggle_stats.clone();
                            overrides.key_toggle_tts = base_config.key_toggle_tts.clone();
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
            Effect::ReturnToStarter => {
                self.save_epub_config();
                self.persist_bookmark();
                self.stop_playback();
                let (next, init_task) = App::bootstrap_starter(self.config.clone());
                *self = next;
                init_task
            }
            Effect::QuitSafely => {
                self.save_epub_config();
                self.persist_bookmark();
                self.stop_playback();
                iced::exit()
            }
        }
    }
}

pub(super) fn runtime_event_to_message(
    event: Event,
    status: event::Status,
    _window_id: window::Id,
) -> Option<Message> {
    if status == event::Status::Captured {
        return match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                Some(Message::AdjustNumericSettingByWheel(wheel_delta_y(delta)))
            }
            _ => None,
        };
    }
    match event {
        Event::Window(iced::window::Event::Resized(size)) => Some(Message::WindowResized {
            width: size.width,
            height: size.height,
        }),
        Event::Window(iced::window::Event::Moved(position)) => Some(Message::WindowMoved {
            x: position.x,
            y: position.y,
        }),
        Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
            Some(Message::KeyPressed { key, modifiers })
        }
        Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
            Some(Message::AdjustNumericSettingByWheel(wheel_delta_y(delta)))
        }
        _ => None,
    }
}

fn wheel_delta_y(delta: mouse::ScrollDelta) -> f32 {
    match delta {
        mouse::ScrollDelta::Lines { y, .. } => y,
        mouse::ScrollDelta::Pixels { y, .. } => y / 40.0,
    }
}

use std::time::Instant;

use lanternleaf_app::contracts::ReaderSnapshot;
use lanternleaf_app::pipeline::{
    AppCommand, DispatchPlan, PersistenceTrigger, PlannedEffect, ReaderCommand, RuntimeEffect,
};
use tracing::trace;

use super::{LanternLeafApp, StatusLogEntry};
use crate::shell::NotificationLevel;

impl LanternLeafApp {
    pub(crate) fn execute_command(&mut self, command: AppCommand) {
        let state_snapshot = self.runtime.state_snapshot();
        let reader_snapshot = state_snapshot.reader_document.snapshot.as_ref();
        self.maybe_record_audio_command(&command, reader_snapshot);
        self.apply_persistence_trigger(&command, reader_snapshot);
        let plan = self.runtime.plan_command(command.clone());
        self.apply_local_events(&plan);
        self.log_plan(&plan);
        self.last_plan = Some(plan);
        if let Some(plan) = &self.last_plan {
            self.dispatch_effects(plan);
        }
        self.apply_tts_command_if_needed(&command);
    }

    pub(crate) fn execute_reader_command(&mut self, command: ReaderCommand) {
        self.execute_command(AppCommand::Reader(command));
    }

    fn apply_local_events(&mut self, plan: &DispatchPlan) {
        for event in &plan.local_events {
            self.runtime.apply_event(event.clone());
        }
    }

    fn dispatch_effects(&self, plan: &DispatchPlan) {
        for effect in &plan.effects {
            self.effect_dispatcher.dispatch(effect.clone());
        }
    }

    fn apply_tts_command_if_needed(&mut self, command: &AppCommand) {
        let AppCommand::Reader(ReaderCommand::Session(session_command)) = command else {
            return;
        };
        let Some(tts_command) =
            lanternleaf_app::tts_runtime::TtsCommand::from_session_command(session_command)
        else {
            return;
        };
        trace!(
            tts_command = tts_command.label(),
            action = session_command.action(),
            "Dispatching TTS command to egui runtime"
        );
        let _ = self.tts_runtime.apply_command(tts_command);
    }

    fn apply_persistence_trigger(
        &mut self,
        command: &AppCommand,
        _snapshot: Option<&ReaderSnapshot>,
    ) {
        let (trigger, description) = match command {
            AppCommand::Reader(_) => (Some(PersistenceTrigger::ReaderCommand), "reader_command"),
            AppCommand::SetRuntimeLogLevel { .. } => (
                Some(PersistenceTrigger::RuntimeConfigChange),
                "runtime_config",
            ),
            AppCommand::SafeQuit | AppCommand::FlushPersistence { .. } => (None, ""),
            _ => (None, ""),
        };
        let Some(trigger) = trigger else {
            return;
        };
        self.record_persistence_event(trigger, description);
        self.queue_persistence_flush(trigger);
    }

    pub(crate) fn queue_persistence_flush(&self, trigger: PersistenceTrigger) {
        let request_id = self.runtime.next_request_id();
        trace!(
            request_id,
            trigger = ?trigger,
            "Queued persistence flush effect"
        );
        self.effect_dispatcher.dispatch(PlannedEffect {
            request_id,
            effect: RuntimeEffect::FlushPersistence { trigger },
        });
    }

    pub(crate) fn update_persistence_lifecycle(&mut self, snapshot: Option<&ReaderSnapshot>) {
        if !self.persistence_logged {
            self.persistence.on_startup();
            self.push_status("Persistence: startup".to_string());
            self.persistence_logged = true;
        }

        match snapshot {
            Some(snapshot) => {
                if self
                    .last_reader_source
                    .as_deref()
                    .map(|path| path != snapshot.source_path)
                    .unwrap_or(true)
                {
                    self.record_persistence_status("source_open", &snapshot.source_path);
                    self.queue_persistence_flush(PersistenceTrigger::SourceOpen);
                    self.last_reader_source = Some(snapshot.source_path.clone());
                }
                self.last_reader_snapshot = Some(snapshot.clone());
            }
            None => {
                if let Some(last_snapshot) = self.last_reader_snapshot.take() {
                    self.record_persistence_status("session_close", &last_snapshot.source_path);
                    self.queue_persistence_flush(PersistenceTrigger::SessionClose);
                }
                self.last_reader_source = None;
            }
        }
    }

    fn record_persistence_status(&mut self, label: &str, source_path: &str) {
        self.push_status(format!("Persistence: {label} ({source_path})"));
    }

    pub(crate) fn handle_effect_events(&mut self) {
        for event in self.effect_dispatcher.drain_events() {
            trace!(event = ?event, "Applying effect event");
            self.runtime.apply_event(event);
        }
    }

    fn log_plan(&mut self, plan: &DispatchPlan) {
        let entry = format!("Planned {} ({})", plan.action, plan.effects.len());
        self.push_status(entry);
    }

    pub(crate) fn push_status(&mut self, message: String) {
        let message_lower = message.to_lowercase();
        let level = if message_lower.contains("error") || message_lower.contains("failed") {
            NotificationLevel::Error
        } else if message_lower.contains("warn") {
            NotificationLevel::Warn
        } else {
            NotificationLevel::Info
        };
        self.status_log.push(StatusLogEntry {
            timestamp: Instant::now(),
            message,
        });
        if let Some(entry) = self.status_log.last() {
            self.shell_state
                .record_notification(level, entry.message.clone());
        }
        if self.status_log.len() > 8 {
            self.status_log.remove(0);
        }
    }
}

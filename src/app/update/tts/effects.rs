use super::super::super::messages::Message;
use super::super::super::state::App;
use super::super::Effect;
use super::transitions::TtsAction;
use iced::Task;

pub(super) fn append_effects_from_actions(actions: Vec<TtsAction>, effects: &mut Vec<Effect>) {
    for action in actions {
        if let TtsAction::DispatchPrepareBatches {
            page,
            request_id,
            audio_start_idx,
            audio_sentences,
        } = action
        {
            effects.push(Effect::PrepareTtsBatches {
                page,
                request_id,
                audio_start_idx,
                audio_sentences,
            });
        }
    }
}

pub(super) fn tasks_from_actions(app: &App, actions: Vec<TtsAction>) -> Task<Message> {
    let mut tasks = Vec::new();

    for action in actions {
        if let TtsAction::SchedulePlan {
            page,
            requested_display_idx,
            request_id,
            display_sentences,
        } = action
        {
            let normalizer = app.normalizer.clone();
            let epub_path = app.epub_path.clone();
            tasks.push(Task::perform(
                async move {
                    let plan = normalizer.plan_page_cached(&epub_path, page, &display_sentences);
                    Message::TtsPlanReady {
                        page,
                        requested_display_idx,
                        request_id,
                        plan,
                    }
                },
                |msg| msg,
            ));
        }
    }

    if tasks.is_empty() {
        Task::none()
    } else {
        Task::batch(tasks)
    }
}

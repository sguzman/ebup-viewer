mod reducer;
mod runtime;
mod shortcuts;

use super::super::messages::Message;
use super::super::state::App;
use iced::event;
use iced::time;
use iced::{Subscription, Task};
use std::time::Duration;

impl App {
    pub fn subscription(app: &App) -> Subscription<Message> {
        let mut subscriptions: Vec<Subscription<Message>> =
            vec![event::listen_with(runtime::runtime_event_to_message)];

        if app.tts.is_playing() {
            subscriptions.push(time::every(Duration::from_millis(80)).map(Message::Tick));
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
}

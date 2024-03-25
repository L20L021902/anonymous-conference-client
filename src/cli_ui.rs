use std::collections::HashMap;

use async_std::{io::BufReader, task};
use async_std::prelude::*;
use futures::channel::mpsc;
use futures::{select, FutureExt, SinkExt};

use crate::constants::MessageID;
use crate::{
    state_manager,
    constants::{
        Receiver,
        Sender,
        UIAction,
        UIEvent,
        ConferenceId,
    },
};

#[allow(non_camel_case_types)]
pub struct CLII_UI {
    ui_event_receiver: Receiver<UIEvent>,
    ui_action_sender: Sender<UIAction>,
    conference_id: Option<ConferenceId>,
    sent_messages: HashMap<MessageID, String>,
    last_message_id: MessageID,
    can_send_messages: bool,
}

impl CLII_UI {
    pub fn new(server_address: String) -> Self {
        let (ui_event_sender, ui_event_receiver) = mpsc::unbounded();
        let (ui_action_sender, ui_action_receiver) = mpsc::unbounded();
        
        // start state manager
        task::spawn(async move {
            state_manager::start_state_manager(server_address, ui_event_sender, ui_action_receiver).await;
        });

        Self {
            ui_event_receiver,
            ui_action_sender,
            conference_id: None,
            sent_messages: HashMap::new(),
            last_message_id: 0,
            can_send_messages: false,
        }
    }

    pub async fn start_ui(&mut self) {
        let mut lines_from_stdin = BufReader::new(async_std::io::stdin()).lines().fuse();

        loop {
            select! {
                line = lines_from_stdin.next().fuse() => match line {
                    Some(line) => {
                        self.process_input(line.unwrap()).await;
                    },
                    None => break,
                },
                ui_event = self.ui_event_receiver.next().fuse() => match ui_event {
                    Some(ui_event) => {
                        self.process_ui_event(ui_event).await;
                    },
                    None => break,
                }
            }

        }
    }

    async fn process_input(&mut self, input: String) {
        let input = input.trim();
        if input.is_empty() {
            return;
        }

        if let Some(input) = input.strip_prefix('/') {
            // command
            let words = input.split_whitespace().collect::<Vec<&str>>();
            match words[0] {
                "create" => {
                    // create conference
                    if words.len() != 2 {
                        self.print_system("Usage: /create <conference password>");
                        return;
                    }
                    let password = words[1].to_string();
                    self.ui_action_sender.send(UIAction::CreateConference(password)).await.unwrap();
                },
                "join" => {
                    // join conference
                    if self.conference_id.is_some() {
                        self.print_system("You are already in a conference. Leave it first.");
                        return;
                    }
                    if words.len() != 3 {
                        self.print_system("Usage: /join <conference id> <conference password>");
                        return;
                    }
                    let Ok(conference_id) = words[1].to_string().parse()
                    else { self.print_system("Invalid conference id"); return; };
                    let password = words[2].to_string();
                    self.ui_action_sender.send(UIAction::JoinConference((conference_id, password))).await.unwrap();
                },
                "leave" => {
                    // leave conference
                    if self.conference_id.is_none() {
                        self.print_system("You are not in a conference.");
                        return;
                    }
                    self.ui_action_sender.send(UIAction::LeaveConference(self.conference_id.unwrap())).await.unwrap();
                },
                "exit" => {
                    // exit
                    self.ui_action_sender.send(UIAction::Disconnect).await.unwrap();
                },
                _ => {
                    self.print_system(format!("Unknown command: /{}", words[0]).as_str());
                },
            }
        } else {
            // text message
            if self.conference_id.is_none() {
                self.print_system("You are not in a conference.");
                return;
            }
            self.last_message_id += 1;
            let message_id = self.last_message_id;
            self.ui_action_sender.send(
                UIAction::SendMessage((self.conference_id.unwrap(), message_id, input.to_string()))
            ).await.unwrap();
            self.sent_messages.insert(message_id, input.to_string());
        }
    }

    async fn process_ui_event(&mut self, ui_event: UIEvent) {
        match ui_event {
            UIEvent::ConferenceCreated(conference_id) => {
                self.print_system(format!("Conference created: {}", conference_id).as_str());
            },
            UIEvent::ConferenceCreateFailed => {
                self.print_system("Failed to create conference.");
            },
            UIEvent::ConferenceJoined((conference_id, number_of_peers)) => {
                self.print_system(format!("Joined conference: {} ({} peers)", conference_id, number_of_peers).as_str());
                self.conference_id = Some(conference_id);
            },
            UIEvent::ConferenceJoinFailed(conference_id) => {
                self.print_system(format!("Failed to join conference: {}", conference_id).as_str());
            },
            UIEvent::ConferenceLeft(conference_id) => {
                self.print_system(format!("Left conference: {}", conference_id).as_str());
                self.conference_id = None;
                self.can_send_messages = false;
            },
            UIEvent::IncomingMessage((_, message, is_signature_valid)) => {
                let message = String::from_utf8_lossy(&message);
                if is_signature_valid {
                    self.print_someone(format!("{}", message).as_str());
                } else {
                    self.print_someone(format!("(!invalid signature!) {}", message).as_str());
                }
            },
            UIEvent::MessageAccepted((_, message_id)) => {
                if let Some(message) = self.sent_messages.get(&message_id) {
                    self.print_you(message);
                    self.sent_messages.remove(&message_id);
                }
            },
            UIEvent::MessageRejected((_, message_id)) => {
                if let Some(message) = self.sent_messages.get(&message_id) {
                    self.print_you(format!("(!server rejected the message!) {}", message).as_str());
                    self.sent_messages.remove(&message_id);
                }
            },
            UIEvent::MessageError((_, message_id)) => {
                if let Some(message) = self.sent_messages.get(&message_id) {
                    self.print_you(format!("(!error sending messsage!) {}", message).as_str());
                    self.sent_messages.remove(&message_id);
                }
            },
            UIEvent::ConferenceRestructuring((_, number_of_peers)) => {
                self.can_send_messages = false;
                self.print_system(format!("Conference restructuring: now has {} peers", number_of_peers).as_str());
            },
            UIEvent::ConferenceRestructuringFinished(_) => {
                self.can_send_messages = true;
                self.print_system("Ready to send messages");
            },
        }
    }

    fn print_system(&self, message: &str) {
        println!("[SYSTEM]: {}", message);
    }

    fn print_someone(&self, message: &str) {
        println!("[SOMEONE]: {}", message);
    }

    fn print_you(&self, message: &str) {
        println!("[YOU]: {}", message);
    }
}



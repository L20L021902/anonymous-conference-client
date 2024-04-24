use std::collections::HashMap;
use crate::constants::{
    ConferenceId, NumberOfPeers, MessageID,
};
use log::debug;
use relm4::{prelude::*, typed_view::list::TypedListView};
use gtk::prelude::*;

use super::message_list_item::{MessageListItem, MessageStatus};

const MESSAGE_INPUT_PLACEHOLDER: &str = "Type your message here...";
const MESSAGE_SEND_BUTTON_TEXT: &str = "Send Message";

pub struct Conference {
    conference_id: ConferenceId,
    number_of_peers: NumberOfPeers,
    conference_id_string: String,
    can_send_messages: bool,
    last_sent_message_id: MessageID,
    sent_messages: HashMap<MessageID, String>,
    messages: TypedListView<MessageListItem, gtk::NoSelection>,
}

#[derive(Debug)]
pub enum ConferenceInput {
    SendMessage(String),
    IncomingMessage((Vec<u8>, bool)),
    MessageAccepted(MessageID),
    MessageRejected(MessageID),
    MessageError(MessageID),
    ConferenceRestructuring(NumberOfPeers),
    ConferenceRestructuringFinished,
}

#[derive(Debug)]
pub enum ConferenceOutput {
    SendMessage((ConferenceId, MessageID, String)),
    LeaveConference(ConferenceId),
}

#[relm4::factory(pub)]
impl FactoryComponent for Conference {
    type Init = (ConferenceId, NumberOfPeers);
    type Input = ConferenceInput;
    type Output = ConferenceOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Stack;
    type Index = String;

    view! {
        #[root]
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_halign: gtk::Align::Fill,
            set_valign: gtk::Align::Fill,
            set_hexpand: true,
            set_spacing: 10,
            set_margin_all: 12,

            // CONFERENCE INFO
            #[name(conference_id_label)]
            gtk::Label {
                set_use_markup: true,
                #[watch]
                set_label: &format!("Conference ID: <b>{}</b>, number of peers: <b>{}</b>", self.conference_id, self.number_of_peers),
            },

            // MESSAGES
            gtk::ScrolledWindow {
                set_vexpand: true,
                set_hexpand: true,

                set_child = Some(&self.messages.view),
            },

            // SEND MESSAGE
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 10,
                set_halign: gtk::Align::Fill,

                #[name(message_input)]
                gtk::Entry {
                    set_placeholder_text: Some(MESSAGE_INPUT_PLACEHOLDER),
                    set_margin_all: 10,
                    set_hexpand: true,
                    connect_activate[send_message_button] => move |_entry| {
                        send_message_button.emit_clicked()
                    }
                },
                #[name(send_message_button)]
                gtk::Button {
                    set_label: MESSAGE_SEND_BUTTON_TEXT,
                    set_margin_all: 10,
                    #[watch]
                    set_sensitive: self.can_send_messages,
                    connect_clicked[message_input] => move |_button| {
                        let message = message_input.text().to_string();
                        if message.is_empty() {
                            return;
                        }
                        message_input.set_text("");
                        sender.input(ConferenceInput::SendMessage(message));
                    }
                }
            }

        },
        #[local_ref]
        returned_widget -> gtk::StackPage {
            set_name: &self.conference_id_string,
            set_title: &self.conference_id_string,
        }
    }

    fn init_model(value: Self::Init, _index: &String, _sender: FactorySender<Self>) -> Self {
        // Initialize the ListView wrapper
        let list_view_wrapper: TypedListView<MessageListItem, gtk::NoSelection> =
            TypedListView::new();

        Self {
            conference_id: value.0,
            number_of_peers: value.1,
            conference_id_string: value.0.to_string(),
            can_send_messages: false,
            last_sent_message_id: 0,
            sent_messages: HashMap::new(),
            messages: list_view_wrapper
        }
    }

    fn update( &mut self, msg: Self::Input, sender: FactorySender<Self>,) -> Self::CommandOutput {
        match msg {
            ConferenceInput::SendMessage(message) => {
                self.last_sent_message_id += 1;
                self.sent_messages.insert(self.last_sent_message_id, message.clone());
                sender.output(ConferenceOutput::SendMessage((self.conference_id, self.last_sent_message_id, message))).unwrap();
            }
            ConferenceInput::IncomingMessage((message, is_signature_valid)) => {
                let message = String::from_utf8_lossy(&message);
                let message_status = if is_signature_valid {
                    MessageStatus::SignatureValid
                } else {
                    MessageStatus::SignatureInvalid
                };
                self.messages.append(MessageListItem::new(false, message.to_string(), message_status));
            }
            ConferenceInput::MessageAccepted(message_id) => {
                if let Some(message) = self.sent_messages.remove(&message_id) {
                    self.messages.append(MessageListItem::new(true, message, MessageStatus::MessageDelivered));
                }
            }
            ConferenceInput::MessageRejected(message_id) => {
                if let Some(message) = self.sent_messages.remove(&message_id) {
                    self.messages.append(MessageListItem::new(true, message, MessageStatus::MessageError));
                }
            }
            ConferenceInput::MessageError(message_id) => {
                if let Some(message) = self.sent_messages.remove(&message_id) {
                    self.messages.append(MessageListItem::new(true, message, MessageStatus::MessageError));
                }
            }
            ConferenceInput::ConferenceRestructuring(new_number_of_peers) => {
                self.number_of_peers = new_number_of_peers;
                self.can_send_messages = false;
            }
            ConferenceInput::ConferenceRestructuringFinished => {
                self.can_send_messages = true;
            }
        }
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        debug!("Conference page with ID {} was destroyed", self.conference_id);
    }
}


use gtk::prelude::*;
use log::debug;
use relm4::factory::{FactoryHashMap, FactoryVecDeque};
use relm4::*;
use crate::constants::{
    ConferenceId, NumberOfPeers, MessageID,
};
use crate::gtk_ui::conference_widget_factory::{ConferenceInput, ConferenceOutput};
use crate::gtk_ui::{
    constants::GUIAction,
    create_conference_frame::CreateConferenceFrame,
    join_conference_frame::JoinConferenceFrame,
    conference_widget_factory::Conference,
};

const ADD_CONFERENCE_PAGE: &str = "add_conference_page";
const ADD_CONFERENCE_PAGE_TEXT: &str = "Add Conference";

pub struct StackWidgets {
    create_conference_frame: Controller<CreateConferenceFrame>,
    join_conference_frame: Controller<JoinConferenceFrame>,
    conferences: FactoryHashMap<String, Conference>,
}

#[derive(Debug)]
pub enum StackAction {
    NewConference((ConferenceId, NumberOfPeers)),
    RemoveConference(ConferenceId),
    ChangedPage,
    IncomingMessage((ConferenceId, Vec<u8>, bool)),
    MessageAccepted((ConferenceId, MessageID)),
    MessageRejected((ConferenceId, MessageID)),
    MessageError((ConferenceId, MessageID)),
    ConferenceRestructuring((ConferenceId, NumberOfPeers)),
    ConferenceRestructuringFinished(ConferenceId),
}

#[relm4::component(pub)]
impl Component for StackWidgets {
    type CommandOutput = ();
    type Init = ();
    type Input = StackAction;
    type Output = GUIAction;

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,

            #[name="stack_switcher"]
            gtk::StackSwitcher {
                set_halign: gtk::Align::Start,
                set_hexpand: true,
                set_stack = Some(stack_widget),
            },
            #[local_ref]
            stack_widget -> gtk::Stack {
                set_transition_type: gtk::StackTransitionType::None,
                set_vexpand: true,
                set_hexpand: true,
                set_valign: gtk::Align::Start,
                connect_visible_child_notify => StackAction::ChangedPage,

                // Add conference page
                add_titled[Some(ADD_CONFERENCE_PAGE), ADD_CONFERENCE_PAGE_TEXT] = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 60,

                    model.create_conference_frame.widget(),
                    model.join_conference_frame.widget(),
                },
            }
        }
    }

    fn init(
        _params: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let create_conference_frame = CreateConferenceFrame::builder().launch(()).forward(sender.output_sender(), |x| x);
        let join_conference_frame = JoinConferenceFrame::builder().launch(()).forward(sender.output_sender(), |x| x);
        let conferences_stack = FactoryHashMap::builder()
            .launch_default()
            .forward(sender.output_sender(), |x| match x {
                ConferenceOutput::SendMessage((conference_id, message_id, message)) => GUIAction::SendMessage((conference_id, message_id, message)),
                ConferenceOutput::LeaveConference(conference_id) => GUIAction::Leave(conference_id),
            });
        let model = StackWidgets {
            create_conference_frame,
            join_conference_frame,
            conferences: conferences_stack,
        };
        let stack_widget = model.conferences.widget();
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            StackAction::NewConference((conference_id, number_of_peers)) => {
                debug!("Added new conference with id: {}", conference_id);
                self.conferences.insert(conference_id.to_string(), (conference_id, number_of_peers));
            }
            StackAction::RemoveConference(conference_id) => {
                debug!("Removed conference with id: {}", conference_id);
                let conference_id_string = conference_id.to_string();
                if self.conferences.keys().any(|x| x == &conference_id_string) {
                    self.conferences.remove(&conference_id_string);
                }
            }
            StackAction::ChangedPage => {
                debug!("Changed page");
            }
            StackAction::IncomingMessage((conference_id, message, signature_valid)) => {
                debug!("Incoming message: {}", conference_id);
                let conference_id_string = conference_id.to_string();
                if self.conferences.keys().any(|x| x == &conference_id_string) {
                    self.conferences.send(&conference_id_string, ConferenceInput::IncomingMessage((message, signature_valid)));
                }
            }
            StackAction::MessageAccepted((conference_id, message_id)) => {
                debug!("Message accepted: {}", conference_id);
                let conference_id_string = conference_id.to_string();
                if self.conferences.keys().any(|x| x == &conference_id_string) {
                    self.conferences.send(&conference_id_string, ConferenceInput::MessageAccepted(message_id));
                }
            }
            StackAction::MessageRejected((conference_id, message_id)) => {
                debug!("Message rejected: {}", conference_id);
                let conference_id_string = conference_id.to_string();
                if self.conferences.keys().any(|x| x == &conference_id_string) {
                    self.conferences.send(&conference_id_string, ConferenceInput::MessageRejected(message_id));
                }
            }
            StackAction::MessageError((conference_id, message_id)) => {
                debug!("Message error: {}", conference_id);
                let conference_id_string = conference_id.to_string();
                if self.conferences.keys().any(|x| x == &conference_id_string) {
                    self.conferences.send(&conference_id_string, ConferenceInput::MessageError(message_id));
                }
            }
            StackAction::ConferenceRestructuring((conference_id, number_of_peers)) => {
                debug!("Conference restructuring: {}", conference_id);
                let conference_id_string = conference_id.to_string();
                if self.conferences.keys().any(|x| x == &conference_id_string) {
                    self.conferences.send(&conference_id_string, ConferenceInput::ConferenceRestructuring(number_of_peers));
                }
            }
            StackAction::ConferenceRestructuringFinished(conference_id) => {
                debug!("Conference restructuring finished: {}", conference_id);
                let conference_id_string = conference_id.to_string();
                if self.conferences.keys().any(|x| x == &conference_id_string) {
                    self.conferences.send(&conference_id_string, ConferenceInput::ConferenceRestructuringFinished);
                }
            }
        }
    }
}

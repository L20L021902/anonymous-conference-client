use async_std::task::{self, JoinHandle};
use futures::{channel::mpsc, SinkExt, StreamExt};
use gtk::prelude::*;
use log::debug;
use relm4::*;
use crate::{
    constants::{
        Receiver, Sender, UIAction, UIEvent, ConferenceId, NumberOfPeers
    },
    state_manager,
    gtk_ui::{
        stack::StackWidgets,
        conference_created_dialog::ConferenceCreatedDialog,
        constants::GUIAction,
    }
};

const APP_ID: &str = "com.anonymous-conference.app";
const MAIN_WINDOW_TITLE_TEXT: &str = "Anonymous Conference Client";

#[derive(PartialEq)]
struct Conference {
    conference_id: ConferenceId,
    number_of_peers: NumberOfPeers,
}

#[tracker::track]
struct AppModel {
    #[do_not_track]
    server_address: String,
    #[do_not_track]
    state_manager_handle: JoinHandle<()>,
    #[do_not_track]
    ui_action_sender: Sender<UIAction>,
    #[do_not_track]
    ui_event_handler_handle: JoinHandle<()>,
    #[do_not_track]
    stack: Controller<StackWidgets>,
    statusbar_string: String,
    conferences: Vec<Conference>,
}

#[relm4::component]
impl SimpleComponent for AppModel {
    /// The type of the messages that this component can receive.
    type Input = GUIAction;
    /// The type of the messages that this component can send.
    type Output = ();
    /// The type of data with which this component will be initialized.
    type Init = String; // server address

    view!{
        #[root]
        gtk::Window {
            set_default_width: 350,
            set_default_height: 350,
            #[wrap(Some)]
            set_titlebar =  &gtk::HeaderBar {
                set_show_title_buttons: true,
                #[wrap(Some)]
                set_title_widget = &gtk::Label {
                    set_text: MAIN_WINDOW_TITLE_TEXT,
                }
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                    
                append = model.stack.widget(),
                #[name="statusbar"]
                append = &gtk::Label {
                    set_halign: gtk::Align::Start,
                    set_margin_all: 5,
                    #[track = "model.changed(AppModel::statusbar_string())"]
                    set_text: &model.statusbar_string,
                }
            }
        }
    }

    /// Initialize the UI and model.
    fn init(
        server_address: Self::Init,
        window: Self::Root,
        sender: relm4::ComponentSender<Self>,
    ) -> relm4::ComponentParts<Self> {
        let (ui_event_sender, ui_event_receiver) = mpsc::unbounded();
        let (ui_action_sender, ui_action_receiver) = mpsc::unbounded();

        let stack = StackWidgets::builder().launch(()).forward(sender.input_sender(), |x| x);
        
        // start state manager
        let server_address_clone = server_address.clone();
        let component_sender_clone = sender.clone();
        let state_manager_handle = task::spawn(async move {
            state_manager::start_state_manager(server_address_clone, ui_event_sender, ui_action_receiver).await;
            debug!("State manager exited");
            component_sender_clone.input(GUIAction::Disconnected);
        });

        // start ui event handler
        let component_sender_clone = sender.clone();
        let ui_event_handler_handle = task::spawn(async move {
            translate_ui_events(ui_event_receiver, component_sender_clone).await;
            debug!("UI event handler exited");
        });

        let model = AppModel {
            server_address,
            state_manager_handle,
            ui_action_sender,
            ui_event_handler_handle,
            stack,
            statusbar_string: "Loading...".to_string(),
            tracker: 0
        };

        let widgets = view_output!();

        relm4::ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: relm4::ComponentSender<Self>) {
        match message {
            GUIAction::Create(password) => {
                debug!("Create conference with password: \"{}\"", password);
                let mut sender_clone = self.ui_action_sender.clone();
                task::spawn(async move {
                    sender_clone.send(UIAction::CreateConference(password)).await.unwrap();
                });
            }
            GUIAction::Join((conference_id, password)) => {
                debug!("Join conference with id: \"{}\" and password: \"{}\"", conference_id, password);
                let mut sender_clone = self.ui_action_sender.clone();
                task::spawn(async move {
                    sender_clone.send(UIAction::JoinConference((conference_id, password))).await.unwrap();
                });
            }
            GUIAction::ConferenceJoined((conference_id, number_of_peers)) => {
                println!("Joined conference with id: \"{}\" and number of peers: \"{}\"", conference_id, number_of_peers);
                self.set_statusbar_string(format!("Joined conference with id: \"{}\" and number of peers: \"{}\"", conference_id, number_of_peers));
                self.stack.input(UIAction::ConferenceJoined((conference_id, number_of_peers)));
            }
            GUIAction::Disconnected => {
                println!("Disconnected from server");
                self.set_statusbar_string("Disconnected from server".to_string());
            }
            _ => {
                println!("I dont now what to do with this signal... {:?}", message);
            }
        }
    }
}

async fn translate_ui_events(mut ui_event_receiver: Receiver<UIEvent>, sender: relm4::ComponentSender<AppModel>) {
    while let Some(ui_event) = ui_event_receiver.next().await {
        match ui_event {
            UIEvent::ConferenceCreated(conference_id) => sender.input(GUIAction::ConferenceCreated(conference_id)),
            UIEvent::ConferenceCreateFailed => sender.input(GUIAction::ConferenceCreateFailed),
            UIEvent::ConferenceJoined((conference_id, number_of_peers)) => sender.input(GUIAction::ConferenceJoined((conference_id, number_of_peers))),
            UIEvent::ConferenceJoinFailed(conference_id) => sender.input(GUIAction::ConferenceJoinFailed(conference_id)),
            UIEvent::ConferenceLeft(conference_id) => sender.input(GUIAction::ConferenceLeft(conference_id)),
            UIEvent::IncomingMessage((conference_id, message, is_private)) => sender.input(GUIAction::IncomingMessage((conference_id, message, is_private))),
            UIEvent::MessageAccepted((conference_id, message_id)) => sender.input(GUIAction::MessageAccepted((conference_id, message_id))),
            UIEvent::MessageRejected((conference_id, message_id)) => sender.input(GUIAction::MessageRejected((conference_id, message_id))),
            UIEvent::MessageError((conference_id, message_id)) => sender.input(GUIAction::MessageError((conference_id, message_id))),
            UIEvent::ConferenceRestructuring((conference_id, number_of_peers)) => sender.input(GUIAction::ConferenceRestructuring((conference_id, number_of_peers))),
            UIEvent::ConferenceRestructuringFinished(conference_id) => sender.input(GUIAction::ConferenceRestructuringFinished(conference_id)),
        }
    }
}

pub fn start_gtk_ui(server_address: String) {
    // Create a new application
    let app = relm4::RelmApp::new(APP_ID);
    app.run::<AppModel>(server_address);
}


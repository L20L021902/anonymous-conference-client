use std::collections::HashMap;

use async_std::task::{self, JoinHandle};
use futures::{channel::mpsc, SinkExt, StreamExt};
use gtk::{glib::property::PropertyGet, prelude::*};
use log::debug;
use relm4::*;
use crate::{
    constants::{
        Receiver, Sender, UIAction, UIEvent, ConferenceId, NumberOfPeers
    },
    state_manager,
    gtk_ui::{
        stack::{StackAction, StackWidgets},
        conference_created_dialog::ConferenceCreatedDialog,
        constants::GUIAction,
    }
};

const APP_ID: &str = "com.anonymous-conference.app";
const MAIN_WINDOW_TITLE_TEXT: &str = "Anonymous Conference Client";

const CONFERENCE_CREATED_DIALOG_TITLE_SUCCESS: &str = "Conference Created";
const CONFERENCE_CREATED_DIALOG_TITLE_ERROR: &str = "Error Creating Conference";
const CONFERENCE_CREATED_DIALOG_TEXT_SUCCESS: &str = "Conference created successfully!\nConference ID is:";
const CONFERENCE_CREATED_DIALOG_TEXT_ERROR: &str = "Error creating conference.\nPlease try again.";

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
    #[do_not_track]
    last_created_conference_password: Option<String>,
}

#[relm4::component]
impl Component for AppModel {
    type CommandOutput = ();
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
            last_created_conference_password: None,
            tracker: 0
        };

        let widgets = view_output!();

        relm4::ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: relm4::ComponentSender<Self>, root: &Self::Root) {
        match message {
            GUIAction::Create(password) => {
                debug!("Create conference with password: \"{}\"", password);
                if self.last_created_conference_password.is_some() {
                    self.set_statusbar_string("Already creating another conference, please wait...".to_string());
                    return;
                }
                self.last_created_conference_password = Some(password.clone());
                let mut sender_clone = self.ui_action_sender.clone();
                task::spawn(async move {
                    sender_clone.send(UIAction::CreateConference(password)).await.unwrap();
                });
            }
            GUIAction::ConferenceCreated(conference_id) => {
                debug!("Conference created with id: \"{}\"", conference_id);
                show_conference_created_success_dialog(conference_id,
                    self.last_created_conference_password.as_ref().unwrap().clone(),
                    sender.clone(),
                    root
                );
                self.last_created_conference_password = None;
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
                self.stack.sender().send(StackAction::NewConference((conference_id, number_of_peers))).unwrap();
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

#[allow(deprecated)]
fn show_conference_created_success_dialog(conference_id: ConferenceId, conference_password: String, sender: relm4::ComponentSender<AppModel>, root: &gtk::Window) {
    let dialog = gtk::MessageDialog::builder()
        .modal(true)
        .transient_for(root)
        .title(CONFERENCE_CREATED_DIALOG_TITLE_SUCCESS)
        .text(format!("{}\n{}", CONFERENCE_CREATED_DIALOG_TEXT_SUCCESS, conference_id))
        .build();
    let dialog_text_label = dialog.message_area().first_child().unwrap();
    let dialog_text = dialog_text_label.downcast_ref::<gtk::Label>().unwrap();
    dialog_text.set_selectable(true);
    dialog_text.set_halign(gtk::Align::Center); // TODO: not working
    dialog.add_button("Close", gtk::ResponseType::Close);
    dialog.add_button("Join Conference", gtk::ResponseType::Apply);
    let sender_clone = sender.clone();
    dialog.connect_response(move |dialog, response_id| {
        match response_id {
            gtk::ResponseType::Close => {
                dialog.close();
            }
            gtk::ResponseType::Apply => {
                sender_clone.input(GUIAction::Join((conference_id, conference_password.clone())));
                dialog.close();
            }
            _ => {}
        }
    });
    dialog.show();
}

pub fn start_gtk_ui(server_address: String) {
    // Create a new application
    let app = relm4::RelmApp::new(APP_ID);
    app.run::<AppModel>(server_address);
}


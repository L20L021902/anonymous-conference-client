use async_std::task::{self, JoinHandle};
use futures::{channel::mpsc, SinkExt, StreamExt};
use gtk::prelude::*;
use log::debug;
use relm4::*;
use crate::{
    constants::{
        Receiver, Sender, UIAction, UIEvent, ConferenceId,
    },
    state_manager,
    gtk_ui::{
        stack::{StackAction, StackWidgets},
        constants::GUIAction,
    }
};

const APP_ID: &str = "com.anonymous-conference.app";
const MAIN_WINDOW_TITLE_TEXT: &str = "Anonymous Conference Client";

const CONFERENCE_CREATED_DIALOG_TITLE_SUCCESS: &str = "Conference Created";
const CONFERENCE_CREATED_DIALOG_TITLE_ERROR: &str = "Error Creating Conference";
const CONFERENCE_CREATED_DIALOG_TEXT_SUCCESS: &str = "Conference created successfully!\nConference ID is:";
const CONFERENCE_CREATED_DIALOG_TEXT_ERROR: &str = "Error creating conference.\nPlease try again.";

const CONFERENCE_JOIN_DIALOG_TITLE_ERROR: &str = "Conference Join Failed";
const CONFERENCE_JOIN_DIALOG_TEXT_ERROR: &str = "Could not join conference, either the conference doesn't exist or the password was incorrect";

const NOT_CONNECTED_TO_SERVER_TITLE: &str = "Failed to connect to the server";
const NOT_CONNECTED_TO_SERVER_TEXT: &str = "Could not connect to the server, make sure that the server is running";

const RECONNECT_BUTTON_TEXT: &str = "Reconnect";

struct AppModel {
    server_address: String,
    state_manager_handle: JoinHandle<()>,
    ui_action_sender: Sender<UIAction>,
    ui_event_handler_handle: JoinHandle<()>,
    stack: Controller<StackWidgets>,
    statusbar_string: String,
    last_created_conference_password: Option<String>,
    ui_event_sender: Sender<UIEvent>,
    reconnect_button_visible: bool,
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
            set_default_width: 700,
            set_default_height: 700,
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
                set_valign: gtk::Align::Fill,
                    
                append = model.stack.widget(),

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 20,
                    #[name="statusbar"]
                    append = &gtk::Label {
                        set_halign: gtk::Align::Start,
                        set_margin_all: 10,
                        #[watch]
                        set_text: &model.statusbar_string,
                    },
                    append = &gtk::Button {
                        #[watch]
                        set_visible: model.reconnect_button_visible,
                        connect_clicked[sender] => move |_| {
                            sender.input(GUIAction::Reconnect)
                        },
                        #[wrap(Some)]
                        set_child = &gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 10,
                            append = &gtk::Label::new(Some(RECONNECT_BUTTON_TEXT)),
                            append = &gtk::Image::from_icon_name("view-refresh"),
                        }
                    }
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

        let stack = StackWidgets::builder().launch(()).forward(sender.input_sender(), |x| x);

        // start state manager
        let (ui_action_sender, ui_action_receiver) = mpsc::unbounded();
        let state_manager_handle = spawn_state_manager(server_address.clone(), sender.clone(), ui_event_sender.clone(), ui_action_receiver);
        let statusbar_string = format!("Connected to {}", &server_address);

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
            statusbar_string,
            last_created_conference_password: None,
            ui_event_sender,
            reconnect_button_visible: false,
        };

        let widgets = view_output!();

        relm4::ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: relm4::ComponentSender<Self>, root: &Self::Root) {
        match message {
            GUIAction::Create(password) => {
                debug!("Create conference with password: \"{}\"", password);
                if self.last_created_conference_password.is_some() {
                    self.statusbar_string = "Already creating another conference, please wait...".to_string();
                    return;
                }
                self.last_created_conference_password = Some(password.clone());
                let mut ui_action_sender_clone = self.ui_action_sender.clone();
                task::spawn(async move {
                    if ui_action_sender_clone.send(UIAction::CreateConference(password)).await.is_err() {
                        sender.input(GUIAction::NotConnectedToServerError);
                    }
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
            GUIAction::ConferenceCreateFailed => {
                debug!("Conference create failed");
                show_simple_dialog(CONFERENCE_CREATED_DIALOG_TITLE_ERROR, CONFERENCE_CREATED_DIALOG_TEXT_ERROR, root);
                self.last_created_conference_password = None;
            }
            GUIAction::Join((conference_id, password)) => {
                debug!("Join conference with id: \"{}\" and password: \"{}\"", conference_id, password);
                let mut sender_clone = self.ui_action_sender.clone();
                task::spawn(async move {
                    if sender_clone.send(UIAction::JoinConference((conference_id, password))).await.is_err() {
                        sender.input(GUIAction::NotConnectedToServerError);
                    }
                });
            }
            GUIAction::ConferenceJoined((conference_id, number_of_peers)) => {
                debug!("Joined conference with id: \"{}\" and number of peers: \"{}\"", conference_id, number_of_peers);
                self.statusbar_string = format!("Joined conference with id: \"{}\" and number of peers: \"{}\"", conference_id, number_of_peers);
                self.stack.sender().send(StackAction::NewConference((conference_id, number_of_peers))).unwrap();
            }
            GUIAction::ConferenceJoinFailed(conference_id) => {
                debug!("Join conference failed, conference ID: {}", conference_id);
                show_simple_dialog(CONFERENCE_JOIN_DIALOG_TITLE_ERROR, CONFERENCE_JOIN_DIALOG_TEXT_ERROR, root);
            }
            GUIAction::SendMessage((conference_id, message_id, message)) => {
                debug!("Sending message in conference with ID: {}", conference_id);
                let mut sender_clone = self.ui_action_sender.clone();
                task::spawn(async move {
                    if sender_clone.send(UIAction::SendMessage((conference_id, message_id, message))).await.is_err() {
                        sender.input(GUIAction::NotConnectedToServerError);
                    }
                });
            }
            GUIAction::Leave(conference_id) => {
                debug!("Leaving conference with ID {}", conference_id);
                let mut sender_clone = self.ui_action_sender.clone();
                task::spawn(async move {
                    if sender_clone.send(UIAction::LeaveConference(conference_id)).await.is_err() {
                        sender.input(GUIAction::NotConnectedToServerError);
                    }
                });
            }
            GUIAction::ConferenceLeft(conference_id) => {
                debug!("Left conference with ID {}", conference_id);
                self.stack.sender().send(StackAction::RemoveConference(conference_id)).unwrap();
                self.statusbar_string = format!("Left conference with id: \"{}\"", conference_id);
            }
            GUIAction::IncomingMessage((conference_id, message, signature_valid)) => {
                debug!("Incoming message in conference with ID: {}", conference_id);
                self.stack.sender().send(StackAction::IncomingMessage((conference_id, message, signature_valid))).unwrap();
            }
            GUIAction::MessageAccepted((conference_id, message_id)) => {
                debug!("Message accepted in conference with ID: {}", conference_id);
                self.stack.sender().send(StackAction::MessageAccepted((conference_id, message_id))).unwrap();
            }
            GUIAction::MessageRejected((conference_id, message_id)) => {
                debug!("Message rejected in conference with ID: {}", conference_id);
                self.stack.sender().send(StackAction::MessageRejected((conference_id, message_id))).unwrap();
            }
            GUIAction::MessageError((conference_id, message_id)) => {
                debug!("Message error in conference with ID: {}", conference_id);
                self.stack.sender().send(StackAction::MessageError((conference_id, message_id))).unwrap();
            }
            GUIAction::ConferenceRestructuring((conference_id, number_of_peers)) => {
                debug!("Conference restructuring in conference with ID: {}", conference_id);
                self.stack.sender().send(StackAction::ConferenceRestructuring((conference_id, number_of_peers))).unwrap();
            }
            GUIAction::ConferenceRestructuringFinished(conference_id) => {
                debug!("Conference restructuring finished in conference with ID: {}", conference_id);
                self.stack.sender().send(StackAction::ConferenceRestructuringFinished(conference_id)).unwrap();
            }
            GUIAction::Disconnected => {
                debug!("Disconnected from server");
                self.statusbar_string = "Disconnected from server".to_string();
                self.stack.sender().send(StackAction::ClearConferences).unwrap();
                self.reconnect_button_visible = true;
            }
            GUIAction::Reconnect => {
                self.reconnect_button_visible = false;
                let (ui_action_sender, ui_action_receiver) = mpsc::unbounded();
                self.ui_action_sender = ui_action_sender;
                self.state_manager_handle = spawn_state_manager(self.server_address.clone(), sender, self.ui_event_sender.clone(), ui_action_receiver);
                debug!("Reconnecting to the server, restarting the state manager...");
                self.statusbar_string = format!("Connected to {}", self.server_address);
            }
            GUIAction::NotConnectedToServerError => {
                debug!("Could not process gui action, not connected to a server");
                show_simple_dialog(NOT_CONNECTED_TO_SERVER_TITLE, NOT_CONNECTED_TO_SERVER_TEXT, root);
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

fn spawn_state_manager(
    server_address: String,
    component_sender: ComponentSender<AppModel>,
    ui_event_sender: Sender<UIEvent>,
    ui_action_receiver: Receiver<UIAction>
) -> JoinHandle<()> {
    task::spawn(async move {
        state_manager::start_state_manager(server_address, ui_event_sender, ui_action_receiver).await;
        debug!("State manager exited");
        component_sender.input(GUIAction::Disconnected);
    })
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

#[allow(deprecated)]
fn show_simple_dialog(title: &str, text: &str, root: &gtk::Window) {
    let dialog = gtk::MessageDialog::builder()
        .modal(true)
        .transient_for(root)
        .title(title)
        .text(text)
        .build();
    dialog.add_button("Close", gtk::ResponseType::Close);
    dialog.connect_response(move |dialog, response_id| {
        if let gtk::ResponseType::Close = response_id {
            dialog.close();
        }
    });
    dialog.show();
}
pub fn start_gtk_ui(server_address: String) {
    // Create a new application
    let random = rand::random::<u32>(); // allow multiple instances
    let app = relm4::RelmApp::new(&format!("{}{}", APP_ID, random));
    app.run::<AppModel>(server_address);
}


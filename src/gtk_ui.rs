use async_std::task::{self, JoinHandle};
use futures::{channel::mpsc, SinkExt, StreamExt};
use gtk4 as gtk;
use gtk::{gio, glib::{self, clone}, prelude::*};
use log::debug;
use relm4::{RelmWidgetExt, SimpleComponent};
use crate::{
    constants::{
        Receiver, Sender, UIAction, UIEvent, ConferenceId, NumberOfPeers, MessageID
    },
    state_manager,
};

const APP_ID: &str = "com.anonymous-conference.app";
const MAIN_WINDOW_TITLE_TEXT: &str = "Anonymous Conference Client";
const ADD_CONFERENCE_PAGE: &str = "add_conference_page";
const ADD_CONFERENCE_PAGE_TEXT: &str = "Add Conference";
const CREATE_CONFERENCE_BUTTON_TEXT: &str = "Create Conference";
const CREATE_CONFERENCE_ENTRY_PLACEHOLDER: &str = "New Conference Password";
const CREATE_CONFERENCE_ENTRY_CHECK_PLACEHOLDER: &str = "New Conference Password Again";
const CREATE_CONFERENCE_ENTRY_ERROR_TOOLTIP: &str = "Passwords are not the same";
const JOIN_CONFERENCE_BUTTON_TEXT: &str = "Join Conference";
const JOIN_CONFERENCE_ENTRY_PLACEHOLDER: &str = "Conference ID";

#[derive(Debug)]
enum GUIAction {
    Create(String),
    Join(String),
    Leave,
    Disconnected,

    ConferenceCreated(ConferenceId),
    ConferenceCreateFailed,
    ConferenceJoined((ConferenceId, NumberOfPeers)),
    ConferenceJoinFailed(ConferenceId),
    ConferenceLeft(ConferenceId),
    IncomingMessage((ConferenceId, Vec<u8>, bool)),
    MessageAccepted((ConferenceId, MessageID)),
    MessageRejected((ConferenceId, MessageID)),
    MessageError((ConferenceId, MessageID)),
    ConferenceRestructuring((ConferenceId, NumberOfPeers)),
    ConferenceRestructuringFinished(ConferenceId),
}

struct State {
    server_address: String,
    state_manager_handle: JoinHandle<()>,
    ui_action_sender: Sender<UIAction>,
    ui_event_handler_handle: JoinHandle<()>,
}

impl State {
    fn new(server_address: String, component_sender: relm4::ComponentSender<State>) -> Self {
        let (ui_event_sender, ui_event_receiver) = mpsc::unbounded();
        let (ui_action_sender, ui_action_receiver) = mpsc::unbounded();
        
        // start state manager
        let server_address_clone = server_address.clone();
        let component_sender_clone = component_sender.clone();
        let state_manager_handle = task::spawn(async move {
            state_manager::start_state_manager(server_address_clone, ui_event_sender, ui_action_receiver).await;
            debug!("State manager exited");
            component_sender_clone.input(GUIAction::Disconnected);
        });

        // start ui event handler
        let component_sender_clone = component_sender.clone();
        let ui_event_handler_handle = task::spawn(async move {
            translate_ui_events(ui_event_receiver, component_sender_clone).await;
            debug!("UI event handler exited");
        });

        Self {
            server_address,
            state_manager_handle,
            ui_action_sender,
            ui_event_handler_handle,
        }
    }
}

struct AppWidgets {
    stack: gtk::Stack,
    statusbar: gtk::Statusbar,
}

impl SimpleComponent for State {
    /// The type of the messages that this component can receive.
    type Input = GUIAction;
    /// The type of the messages that this component can send.
    type Output = ();
    /// The type of data with which this component will be initialized.
    type Init = String; // server address
    /// The root GTK widget that this component will create.
    type Root = gtk::Window;
    /// A data structure that contains the widgets that you will need to update.
    type Widgets = AppWidgets;

    fn init_root() -> Self::Root {
        gtk::Window::builder()
            .title(MAIN_WINDOW_TITLE_TEXT)
            .default_width(350)
            .default_height(350)
            .build()
    }

     /// Initialize the UI and model.
    fn init(
        server_address: Self::Init,
        window: Self::Root,
        sender: relm4::ComponentSender<Self>,
    ) -> relm4::ComponentParts<Self> {
        let state = State::new(server_address, sender.clone());

        // HEADER BAR
        make_headerbar(&window);

        // MAIN CONTENT
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let (stack_switcher, stack) = make_stack(sender);
        vbox.append(&stack_switcher);
        vbox.append(&stack);
        let statusbar = gtk::Statusbar::new();
        vbox.append(&statusbar);
        let context_id = statusbar.context_id("status");
        println!("context_id: {}", context_id);
        statusbar.push(context_id, "Loading...");

        window.set_child(Some(&vbox));

        let widgets = AppWidgets { stack, statusbar };

        relm4::ComponentParts { model: state, widgets }
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
            GUIAction::Disconnected => {
                println!("Disconnected from server");
            }
            _ => {
                println!("I dont now what to do with this signal... {:?}", message);
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: relm4::ComponentSender<Self>) {
        println!("updated view...");
    }
}

async fn translate_ui_events(mut ui_event_receiver: Receiver<UIEvent>, sender: relm4::ComponentSender<State>) {
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
    app.run::<State>(server_address);
}

fn make_headerbar(window: &gtk::Window) {
    let headerbar = gtk::HeaderBar::new();
    headerbar.set_show_title_buttons(true);

    let title_label = gtk::Label::new(Some("Anonymous Conference Client"));
    headerbar.set_title_widget(Some(&title_label));

    window.set_titlebar(Some(&headerbar));
}

fn make_stack(sender: relm4::ComponentSender<State>) -> (gtk::StackSwitcher, gtk::Stack) {
    let stack_switcher = gtk::StackSwitcher::new();
    stack_switcher.set_halign(gtk::Align::Start);
    stack_switcher.set_hexpand(true);

    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::None);
    stack.set_vexpand(true);
    stack.set_valign(gtk::Align::Center);

    stack_switcher.set_stack(Some(&stack));

    let add_conference_box = make_add_conference_page(sender);
    let add_conference_page = stack.add_titled(&add_conference_box, Some(ADD_CONFERENCE_PAGE), ADD_CONFERENCE_PAGE_TEXT);

    let label2 = gtk::Label::new(Some("Test2"));
    let add_conference_page2 = stack.add_titled(&label2, Some("test2"), "Add Conference2");

    (stack_switcher, stack)
}

fn make_add_conference_page(sender: relm4::ComponentSender<State>) -> gtk::Box {
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 60);

    let create_conference_frame = make_create_conference_frame(&sender);
    vbox.append(&create_conference_frame);

    let join_conference_frame = make_join_conference_frame(&sender);
    vbox.append(&join_conference_frame);

    vbox
}

fn make_join_conference_frame(sender: &relm4::ComponentSender<State>) -> gtk::Frame {
    let frame = gtk::Frame::new(Some(JOIN_CONFERENCE_BUTTON_TEXT));
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 10);
    frame.set_child(Some(&vbox));

    frame.set_halign(gtk::Align::Center);
    frame.set_width_request(300);
    vbox.set_margin_all(10);

    let join_conference_button = gtk::Button::with_label(JOIN_CONFERENCE_BUTTON_TEXT);
    let join_conference_entry = gtk::Entry::new();
    vbox.append(&join_conference_button);
    vbox.append(&join_conference_entry);

    join_conference_button.set_sensitive(false); // disable button until text is entered

    join_conference_entry.set_placeholder_text(Some(JOIN_CONFERENCE_ENTRY_PLACEHOLDER));
    join_conference_entry.set_max_length(10); // u32::MAX character len
    gtk::prelude::EntryExt::set_alignment(&join_conference_entry, 0.5); // center text
    let pattern = |c: char| !c.is_numeric();
    join_conference_entry.delegate().unwrap().connect_insert_text(move |entry, text, position| {
        if text.contains(pattern) {
            glib::signal::signal_stop_emission_by_name(entry, "insert-text");
            entry.insert_text(&text.replace(pattern, ""), position);
        }
    });

    let join_conference_button_clone = join_conference_button.clone();
    join_conference_entry.connect_changed(move |entry| {
        join_conference_button_clone.set_sensitive(!entry.text().is_empty());
    });

    join_conference_button.connect_clicked(clone!(@strong sender => move |_| {
        let text = join_conference_entry.text().to_string();
        join_conference_entry.set_text("");
        sender.input(GUIAction::Join(text));
    }));

    frame
}

fn make_create_conference_frame(sender: &relm4::ComponentSender<State>) -> gtk::Frame {
    let frame = gtk::Frame::new(Some(CREATE_CONFERENCE_BUTTON_TEXT));
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 10);
    frame.set_child(Some(&vbox));

    frame.set_halign(gtk::Align::Center);
    frame.set_width_request(300);
    vbox.set_margin_all(10);

    let create_conference_button = gtk::Button::with_label(CREATE_CONFERENCE_BUTTON_TEXT);
    let create_conference_entry = gtk::Entry::new();
    let create_conference_entry_check = gtk::Entry::new();
    vbox.append(&create_conference_button);
    vbox.append(&create_conference_entry);
    vbox.append(&create_conference_entry_check);

    create_conference_button.set_sensitive(false); // disable button until text is entered

    create_conference_entry.set_placeholder_text(Some(CREATE_CONFERENCE_ENTRY_PLACEHOLDER));
    create_conference_entry_check.set_placeholder_text(Some(CREATE_CONFERENCE_ENTRY_CHECK_PLACEHOLDER));
    create_conference_entry.set_visibility(false);
    create_conference_entry_check.set_visibility(false);

    let create_conference_button_clone = create_conference_button.clone();
    let create_conference_entry_check_clone = create_conference_entry_check.clone();
    create_conference_entry.connect_changed(move |entry| {
        if entry.text().is_empty() || create_conference_entry_check_clone.text().is_empty() {
            show_password_error_tooltip(entry, false);
            show_password_error_tooltip(&create_conference_entry_check_clone, false);
            create_conference_button_clone.set_sensitive(false);
            return;
        }
        if entry.text() == create_conference_entry_check_clone.text() {
            show_password_error_tooltip(entry, false);
            show_password_error_tooltip(&create_conference_entry_check_clone, false);
            create_conference_button_clone.set_sensitive(true);
        } else {
            show_password_error_tooltip(entry, true);
            show_password_error_tooltip(&create_conference_entry_check_clone, true);
            create_conference_button_clone.set_sensitive(false);
        }
    });

    let create_conference_button_clone = create_conference_button.clone();
    let create_conference_entry_clone = create_conference_entry.clone();
    create_conference_entry_check.connect_changed(move |entry| {
        if entry.text().is_empty() || create_conference_entry_clone.text().is_empty() {
            show_password_error_tooltip(entry, false);
            show_password_error_tooltip(&create_conference_entry_clone, false);
            create_conference_button_clone.set_sensitive(false);
            return;
        }
        if entry.text() == create_conference_entry_clone.text() {
            show_password_error_tooltip(entry, false);
            show_password_error_tooltip(&create_conference_entry_clone, false);
            create_conference_button_clone.set_sensitive(true);
        } else {
            show_password_error_tooltip(entry, true);
            show_password_error_tooltip(&create_conference_entry_clone, true);
            create_conference_button_clone.set_sensitive(false);
        }
    });

    create_conference_button.connect_clicked(clone!(@strong sender => move |_| {
        let text = create_conference_entry.text().to_string();
        create_conference_entry.set_text("");
        create_conference_entry_check.set_text("");
        sender.input(GUIAction::Create(text));
    }));

    frame
}

fn show_password_error_tooltip(entry: &gtk::Entry, show: bool) {
    if show {
        // Set error icon
        entry.set_icon_from_icon_name(
            gtk::EntryIconPosition::Secondary,
            Some("dialog-error"),
        );

        // Set tooltip text for error icon
        entry.set_icon_tooltip_text(
            gtk::EntryIconPosition::Secondary,
            Some(CREATE_CONFERENCE_ENTRY_ERROR_TOOLTIP),
        );
    } else {
        // Remove error icon and tooltip text
        entry.set_icon_from_icon_name(
            gtk::EntryIconPosition::Secondary,
            None::<&str>,
        );

        entry.set_icon_tooltip_text(
            gtk::EntryIconPosition::Secondary,
            None::<&str>,
        );
    }
}

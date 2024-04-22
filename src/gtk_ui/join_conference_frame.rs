use gtk::{glib, prelude::*};
use relm4::*;
use crate::gtk_ui::constants::GUIAction;

const JOIN_CONFERENCE_BUTTON_TEXT: &str = "Join Conference";
const JOIN_CONFERENCE_ENTRY_PLACEHOLDER: &str = "Conference ID";
const JOIN_CONFERENCE_ENTRY_PASSWORD_PLACEHOLDER: &str = "Conference Password";

pub struct JoinConferenceFrame;

#[relm4::component(pub)]
impl SimpleComponent for JoinConferenceFrame {
    type Init = ();
    type Input = ();
    type Output = GUIAction;

    view! {
        #[root]
        gtk::Frame {
            set_label: Some(JOIN_CONFERENCE_BUTTON_TEXT),
            set_halign: gtk::Align::Center,
            set_width_request: 300,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 10,

                #[name="join_conference_button"]
                append = &gtk::Button {
                    set_label: JOIN_CONFERENCE_BUTTON_TEXT,
                    set_sensitive: false,
                    connect_clicked[sender, join_conference_entry, join_conference_entry_password] => move |_| {
                        let conference_id = join_conference_entry.text().to_string().parse().unwrap(); // entry should only contain numbers
                        let conference_password = join_conference_entry_password.text().to_string();
                        join_conference_entry.set_text("");
                        join_conference_entry_password.set_text("");
                        sender.output(GUIAction::Join((conference_id, conference_password))).unwrap();
                    }
                },
                #[name="join_conference_entry"]
                append = &gtk::Entry {
                    set_placeholder_text: Some(JOIN_CONFERENCE_ENTRY_PLACEHOLDER),
                    set_max_length: 10, // u32::MAX character len
                    EntryExt::set_alignment: 0.5,
                    connect_changed[join_conference_button, join_conference_entry_password] => move |entry| {
                        join_conference_button.set_sensitive(!entry.text().is_empty() && !join_conference_entry_password.text().is_empty());
                    },
                },
                #[name="join_conference_entry_password"]
                append = &gtk::Entry {
                    set_placeholder_text: Some(JOIN_CONFERENCE_ENTRY_PASSWORD_PLACEHOLDER),
                    set_visibility: false,
                    EntryExt::set_alignment: 0.5,
                    connect_changed[join_conference_button, join_conference_entry] => move |entry| {
                        join_conference_button.set_sensitive(!entry.text().is_empty() && !join_conference_entry.text().is_empty());
                    },
                }
            }
        }
    }

    fn init(
        _params: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self;
        let widgets = view_output!();
        widgets.join_conference_entry.delegate().unwrap().connect_insert_text(move |entry, text, position| {
            if text.chars().any(|c| !c.is_numeric()) {
                glib::signal::signal_stop_emission_by_name(entry, "insert-text");
                entry.insert_text(&text.chars().filter(|c| c.is_numeric()).collect::<String>(), position);
            }
        }); // TODO: move to view! macro if possible
        ComponentParts { model, widgets }
    }
}

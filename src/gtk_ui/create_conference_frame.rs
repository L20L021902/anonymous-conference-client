use gtk::prelude::*;
use relm4::*;
use crate::gtk_ui::constants::GUIAction;

const CREATE_CONFERENCE_BUTTON_TEXT: &str = "Create Conference";
const CREATE_CONFERENCE_ENTRY_PLACEHOLDER: &str = "New Conference Password";
const CREATE_CONFERENCE_ENTRY_CHECK_PLACEHOLDER: &str = "New Conference Password Again";
const CREATE_CONFERENCE_ENTRY_ERROR_TOOLTIP: &str = "Passwords are not the same";

pub struct CreateConferenceFrame;

#[relm4::component(pub)]
impl SimpleComponent for CreateConferenceFrame {
    type Init = ();
    type Input = ();
    type Output = GUIAction;

    view! {
        #[root]
        gtk::Frame {
            set_label: Some(CREATE_CONFERENCE_BUTTON_TEXT),
            set_halign: gtk::Align::Center,
            set_width_request: 300,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 10,
                set_margin_all: 10,

                #[name="create_conference_button"]
                append = &gtk::Button {
                    set_label: CREATE_CONFERENCE_BUTTON_TEXT,
                    set_sensitive: false,
                    connect_clicked[sender, create_conference_entry, create_conference_entry_check] => move |_| {
                        let text = create_conference_entry.text().to_string();
                        create_conference_entry.set_text("");
                        create_conference_entry_check.set_text("");
                        sender.output(GUIAction::Create(text)).unwrap();
                    }
                },
                #[name="create_conference_entry"]
                append = &gtk::Entry {
                    set_placeholder_text: Some(CREATE_CONFERENCE_ENTRY_PLACEHOLDER),
                    set_visibility: false,
                    connect_changed[create_conference_button, create_conference_entry_check] => move |entry| {
                        if entry.text().is_empty() || create_conference_entry_check.text().is_empty() {
                            show_password_error_tooltip(entry, false);
                            show_password_error_tooltip(&create_conference_entry_check, false);
                            create_conference_button.set_sensitive(false);
                            return;
                        }
                        if entry.text() == create_conference_entry_check.text() {
                            show_password_error_tooltip(entry, false);
                            show_password_error_tooltip(&create_conference_entry_check, false);
                            create_conference_button.set_sensitive(true);
                        } else {
                            show_password_error_tooltip(entry, true);
                            show_password_error_tooltip(&create_conference_entry_check, true);
                            create_conference_button.set_sensitive(false);
                        }
                    },
                },
                #[name="create_conference_entry_check"]
                append = &gtk::Entry {
                    set_placeholder_text: Some(CREATE_CONFERENCE_ENTRY_CHECK_PLACEHOLDER),
                    set_visibility: false,
                    connect_changed[create_conference_button, create_conference_entry] => move |entry| {
                        if entry.text().is_empty() || create_conference_entry.text().is_empty() {
                            show_password_error_tooltip(entry, false);
                            show_password_error_tooltip(&create_conference_entry, false);
                            create_conference_button.set_sensitive(false);
                            return;
                        }
                        if entry.text() == create_conference_entry.text() {
                            show_password_error_tooltip(entry, false);
                            show_password_error_tooltip(&create_conference_entry, false);
                            create_conference_button.set_sensitive(true);
                        } else {
                            show_password_error_tooltip(entry, true);
                            show_password_error_tooltip(&create_conference_entry, true);
                            create_conference_button.set_sensitive(false);
                        }
                    },
                },
            }
        },
    }

    fn init(
        _params: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self;
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }
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

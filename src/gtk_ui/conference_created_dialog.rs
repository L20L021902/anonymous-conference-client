use relm4::{SimpleComponent, ComponentSender, ComponentParts};
use gtk::prelude::*;

const CONFERENCE_CREATED_DIALOG_TITLE_SUCCESS: &str = "Conference Created";
const CONFERENCE_CREATED_DIALOG_TITLE_FAILURE: &str = "Conference Creation Failed";
const CONFERENCE_CREATED_DIALOG_TEXT_SUCCESS: &str = "Conference ID is ";
const CONFERENCE_CREATED_DIALOG_TEXT_FAILURE: &str = "Conference creation failed";
const CONFERENCE_CREATED_DIALOG_BUTTON_CLOSE_TEXT: &str = "Close";
const CONFERENCE_CREATED_DIALOG_BUTTON_AUTOJOIN_TEXT: &str = "Join Created Conference";

#[derive(Debug)]
pub enum DialogInput {
    Show(Option<String>),
    AutoJoin,
    Close
}

#[derive(Debug)]
pub enum DialogOutput {
    AutoJoin,
}

pub struct ConferenceCreatedDialog {
    hidden: bool,
    conference_id: Option<String>,
    success_text: Option<String>,
}

#[relm4::component(pub)]
impl SimpleComponent for ConferenceCreatedDialog {
    type Init = ();
    type Input = DialogInput;
    type Output = DialogOutput;

    view! {
        #[name="dialog"]
        gtk::MessageDialog {
            set_modal: true,
            #[watch]
            set_visible: !model.hidden,
            #[watch]
            set_text: Some(
                if model.conference_id.is_some() { CONFERENCE_CREATED_DIALOG_TITLE_SUCCESS } else { CONFERENCE_CREATED_DIALOG_TITLE_FAILURE }
            ),
            #[watch]
            set_secondary_text: Some(
                if model.conference_id.is_some() { model.success_text.as_ref().unwrap() } else { CONFERENCE_CREATED_DIALOG_TEXT_FAILURE }
            ),
            add_button: (CONFERENCE_CREATED_DIALOG_BUTTON_CLOSE_TEXT, gtk::ResponseType::Cancel),
            connect_response[sender] => move |_, resp| {
                sender.input(if resp == gtk::ResponseType::Accept {
                    DialogInput::AutoJoin
                } else {
                    DialogInput::Close
                })
            }
        }
    }

    fn init(
        params: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            hidden: false,
            success_text: None,
            conference_id: None,
        };
        let widgets = view_output!();
        // if self.conference_id.is_some() {
        //     widgets.dialog.add_button(CONFERENCE_CREATED_DIALOG_BUTTON_AUTOJOIN_TEXT, gtk::ResponseType::Accept);
        // }
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            DialogInput::Show(conference_id) => {
                self.hidden = false
            },
            DialogInput::AutoJoin => {
                self.hidden = true;
                sender.output(DialogOutput::AutoJoin).unwrap()
            }
            DialogInput::Close => self.hidden = true,
        }
    }
}

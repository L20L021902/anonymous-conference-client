use gtk::prelude::*;
use relm4::*;
use crate::constants::{
    ConferenceId, NumberOfPeers,
};
use crate::gtk_ui::{
    constants::GUIAction,
    create_conference_frame::CreateConferenceFrame,
    join_conference_frame::JoinConferenceFrame,
};

const ADD_CONFERENCE_PAGE: &str = "add_conference_page";
const ADD_CONFERENCE_PAGE_TEXT: &str = "Add Conference";

#[derive(PartialEq)]
struct Conference {
    conference_id: ConferenceId,
    number_of_peers: NumberOfPeers,
}

#[tracker::track]
pub struct StackWidgets {
    #[do_not_track]
    create_conference_frame: Controller<CreateConferenceFrame>,
    #[do_not_track]
    join_conference_frame: Controller<JoinConferenceFrame>,
    conferences: Vec<Conference>,
}

#[derive(Debug)]
pub enum StackAction {
    NewConference((ConferenceId, NumberOfPeers)),
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
            append = &gtk::StackSwitcher {
                set_halign: gtk::Align::Start,
                set_hexpand: true,
                // set_stack = Some(&stack),
            },
            #[name="stack"]
            append = &gtk::Stack {
                set_transition_type: gtk::StackTransitionType::None,
                set_vexpand: true,
                set_valign: gtk::Align::Center,

                // Add conference page
                add_titled[Some(ADD_CONFERENCE_PAGE), ADD_CONFERENCE_PAGE_TEXT] = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 60,
                    append = model.create_conference_frame.widget(),
                    append = model.join_conference_frame.widget(),
                },
                // Conferences pages
            }
        }
    }

    fn init(
        _params: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = StackWidgets {
            create_conference_frame: CreateConferenceFrame::builder().launch(()).forward(sender.output_sender(), |x| x),
            join_conference_frame: JoinConferenceFrame::builder().launch(()).forward(sender.output_sender(), |x| x),
            conferences: Vec::new(),
            tracker: 0
        };
        let widgets = view_output!();
        widgets.stack_switcher.set_stack(Some(&widgets.stack)); // TODO: move it to view! macro
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, root: &Self::Root) {
        match msg {
            StackAction::NewConference((conference_id, number_of_peers)) => {
                self.conferences.push(Conference { conference_id, number_of_peers });
            }
        }
    }
}

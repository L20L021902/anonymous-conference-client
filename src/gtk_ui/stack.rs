use gtk::prelude::*;
use relm4::*;
use crate::gtk_ui::{
    constants::GUIAction,
    create_conference_frame::CreateConferenceFrame,
    join_conference_frame::JoinConferenceFrame,
};

const ADD_CONFERENCE_PAGE: &str = "add_conference_page";
const ADD_CONFERENCE_PAGE_TEXT: &str = "Add Conference";

pub struct StackWidgets {
    create_conference_frame: Controller<CreateConferenceFrame>,
    join_conference_frame: Controller<JoinConferenceFrame>,
}

#[relm4::component(pub)]
impl SimpleComponent for StackWidgets {
    type Init = ();
    type Input = ();
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
        };
        let widgets = view_output!();
        widgets.stack_switcher.set_stack(Some(&widgets.stack)); // TODO: move it to view! macro
        ComponentParts { model, widgets }
    }
}

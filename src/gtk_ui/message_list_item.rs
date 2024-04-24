use gtk::prelude::*;
use relm4::{
    binding::{Binding, U8Binding},
    prelude::*,
    typed_view::list::{RelmListItem, TypedListView},
    RelmObjectExt,
    view,
};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessageStatus {
    SignatureValid,
    SignatureInvalid,
    MessageDelivered,
    MessageError,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MessageListItem {
    sent_by_me: bool,
    text: String,
    status: MessageStatus,
    binding: U8Binding, // MessageID is 32 bytes
}


impl MessageListItem {
    pub fn new(sent_by_me: bool, text: String, status: MessageStatus) -> Self {
        Self {
            sent_by_me,
            text,
            status,
            binding: U8Binding::new(0),
        }
    }
}

pub struct MessageWidgets {
    author: gtk::Label,
    text: gtk::Label,
    status: gtk::Label,
}

impl RelmListItem for MessageListItem {
    type Root = gtk::Box;
    type Widgets = MessageWidgets;

    fn setup(_item: &gtk::ListItem) -> (Self::Root, Self::Widgets) {
        view! {
            hbox = gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 10,
                set_margin_all: 10,
                set_hexpand: true,

                #[name(author)]
                gtk::Label {
                    set_halign: gtk::Align::Start,
                    set_valign: gtk::Align::Center,
                },
                #[name(text)]
                gtk::Label {
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                    set_valign: gtk::Align::Center,
                },
                #[name(status)]
                gtk::Label {
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::Center,
                },
            }

        }

        let widgets = Self::Widgets {
            author,
            text,
            status,
        };

        (hbox, widgets)
    }

    fn bind(&mut self, widgets: &mut Self::Widgets, _root: &mut Self::Root) {
        let Self::Widgets {
            author,
            text,
            status,
        } = widgets;

        if self.sent_by_me {
            author.set_text("YOU:")
        } else {
            author.set_text("SOMEONE:")
        }

        text.set_text(&self.text);

        match self.status {
            MessageStatus::SignatureValid => status.set_text("VALID"),
            MessageStatus::SignatureInvalid => status.set_text("INVALID"),
            MessageStatus::MessageDelivered => status.set_text("DELIVERED"),
            MessageStatus::MessageError => status.set_text("ERROR"),
        }
    }
}


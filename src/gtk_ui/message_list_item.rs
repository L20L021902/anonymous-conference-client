use gtk::prelude::*;
use relm4::{
    binding::U8Binding,
    prelude::*,
    typed_view::list::RelmListItem,
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
    status: gtk::Image,
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
                add_css_class: "message-box",
                

                #[name(author)]
                gtk::Label {
                    set_halign: gtk::Align::Start,
                    set_valign: gtk::Align::Start,
                },
                #[name(text)]
                gtk::Label {
                    set_hexpand: true,
                    set_wrap: true,
                    set_wrap_mode: gtk::pango::WrapMode::WordChar,
                    set_halign: gtk::Align::Start,
                    set_valign: gtk::Align::Center,
                },
                #[name(status)]
                gtk::Image {
                    set_valign: gtk::Align::End,
                }
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
            MessageStatus::SignatureValid => status.set_from_icon_name(Some("security-high")),
            MessageStatus::SignatureInvalid => status.set_from_icon_name(Some("security-low")),
            MessageStatus::MessageDelivered => status.set_from_icon_name(Some("emblem-ok")),
            MessageStatus::MessageError => status.set_from_icon_name(Some("emblem-unreadable")),
        }
    }
}


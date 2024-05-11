use crate::{
    constants::{
        ConferenceId, NumberOfPeers, MessageID,
    }
};

#[derive(Debug)]
pub enum GUIAction {
    Create(String),
    Join((ConferenceId, String)),
    Leave(ConferenceId),
    SendMessage((ConferenceId, MessageID, String)),
    Disconnected,
    Reconnect,
    NotConnectedToServerError,

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

use crate::constants::{
    Receiver,
    Sender,
    Result,
    ServerEvent,
    UIEvent,
    ConferenceId,
    NumberOfPeers,
    EncryptionKey,
    Message,
};

enum ConferenceState {
    Initial,
    PublicKeyExchange,
    PublicKeyExchangeFinished,
    EncryptionKeyNegotiation,
    EncryptionKeyNegotiationFinished,
    NormalOperation,
}

pub fn start_conference_manager(
    conference_id: ConferenceId,
    number_of_peers: NumberOfPeers,
    initial_encryption_key: EncryptionKey,
    conference_event_receiver: Receiver<ServerEvent>,
    message_sender: Sender<Message>,
    ui_event_sender: Sender<UIEvent>,
) -> Result<()> {
    let mut state = ConferenceState::Initial;
    todo!();
}

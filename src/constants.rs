use futures::channel::mpsc;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub type Sender<T> = mpsc::UnboundedSender<T>;
pub type Receiver<T> = mpsc::UnboundedReceiver<T>;

pub type ConferenceId = u32;
pub type NumberOfPeers = u32;
pub type EncryptionKey = [u8; 32];
pub type PacketNonce = u32;
pub type MessageLength = u32;
pub type PasswordHash = [u8; 32];
pub type ConferenceJoinSalt = [u8; 32];
pub type ConferenceEncryptionSalt = [u8; 32];


#[derive(Clone)]
pub struct Message {
    pub conference: ConferenceId,
    pub message: Vec<u8>,
    pub message_id: Option<MessageID>,
}

#[repr(u8)]
#[derive(Clone)]
pub enum ClientEvent {
    CreateConference((PacketNonce, PasswordHash, ConferenceJoinSalt, ConferenceEncryptionSalt)) = 0x01,
    GetConferenceJoinSalt((PacketNonce, ConferenceId)) = 0x02,
    JoinConference((PacketNonce, ConferenceId, PasswordHash)) = 0x03,
    LeaveConference((PacketNonce, ConferenceId)) = 0x04,
    SendMessage((PacketNonce, Message)) = 0x05,
    Disconnect = 0x06,
}

impl ClientEvent {
    pub fn value(&self) -> u8 {
        unsafe { *(self as *const Self as *const u8) }
    }
}

#[repr(u8)]
pub enum ServerEvent {
    HandshakeAcknowledged = 0x00,
    ConferenceCreated((PacketNonce, ConferenceId)) = 0x01,
    ConferenceJoinSalt((PacketNonce, ConferenceId, ConferenceJoinSalt)) = 0x02,
    ConferenceJoined((PacketNonce, ConferenceId, NumberOfPeers, ConferenceEncryptionSalt)) = 0x03,
    ConferenceLeft((PacketNonce, ConferenceId)) = 0x04,
    MessageAccepted((PacketNonce, ConferenceId)) = 0x05,
    ConferenceRestructuring((ConferenceId, NumberOfPeers)) = 0x06,
    IncomingMessage((ConferenceId, Vec<u8>)) = 0x07,

    GeneralError = 0x10,
    ConferenceCreationError(PacketNonce) = 0x11,
    ConferenceJoinSaltError((PacketNonce, ConferenceId)) = 0x12,
    ConferenceJoinError((PacketNonce, ConferenceId)) = 0x13,
    ConferenceLeaveError((PacketNonce, ConferenceId)) = 0x14,
    MessageError((PacketNonce, ConferenceId)) = 0x15,
}

pub enum ConferenceEvent {
    ConferenceRestructuring(NumberOfPeers),
    IncomingMessage(Vec<u8>),
    OutboundMessage((MessageID, Vec<u8>)),
}

#[repr(u8)]
pub enum ServerToClientMessageTypePrimitive {
    HandshakeAcknowledged = 0x00,
    ConferenceCreated = 0x01,
    ConferenceJoinSalt = 0x02,
    ConferenceJoined = 0x03,
    ConferenceLeft = 0x04,
    MessageAccepted = 0x05,
    ConferenceRestructuring = 0x06,
    IncomingMessage = 0x07,

    GeneralError = 0x10,
    ConferenceCreationError = 0x11,
    ConferenceJoinSaltError = 0x12,
    ConferenceJoinError = 0x13,
    ConferenceLeaveError = 0x14,
    MessageError = 0x15,
}

impl TryFrom<u8> for ServerToClientMessageTypePrimitive {
    type Error = ();

    fn try_from(v: u8) -> std::result::Result<Self, Self::Error> {
        match v {
            x if x == ServerToClientMessageTypePrimitive::HandshakeAcknowledged as u8 => Ok(ServerToClientMessageTypePrimitive::HandshakeAcknowledged),
            x if x == ServerToClientMessageTypePrimitive::ConferenceCreated as u8 => Ok(ServerToClientMessageTypePrimitive::ConferenceCreated),
            x if x == ServerToClientMessageTypePrimitive::ConferenceJoinSalt as u8 => Ok(ServerToClientMessageTypePrimitive::ConferenceJoinSalt),
            x if x == ServerToClientMessageTypePrimitive::ConferenceJoined as u8 => Ok(ServerToClientMessageTypePrimitive::ConferenceJoined),
            x if x == ServerToClientMessageTypePrimitive::ConferenceLeft as u8 => Ok(ServerToClientMessageTypePrimitive::ConferenceLeft),
            x if x == ServerToClientMessageTypePrimitive::MessageAccepted as u8 => Ok(ServerToClientMessageTypePrimitive::MessageAccepted),
            x if x == ServerToClientMessageTypePrimitive::ConferenceRestructuring as u8 => Ok(ServerToClientMessageTypePrimitive::ConferenceRestructuring),
            x if x == ServerToClientMessageTypePrimitive::IncomingMessage as u8 => Ok(ServerToClientMessageTypePrimitive::IncomingMessage),

            x if x == ServerToClientMessageTypePrimitive::GeneralError as u8 => Ok(ServerToClientMessageTypePrimitive::GeneralError),
            x if x == ServerToClientMessageTypePrimitive::ConferenceCreationError as u8 => Ok(ServerToClientMessageTypePrimitive::ConferenceCreationError),
            x if x == ServerToClientMessageTypePrimitive::ConferenceJoinSaltError as u8 => Ok(ServerToClientMessageTypePrimitive::ConferenceJoinSaltError),
            x if x == ServerToClientMessageTypePrimitive::ConferenceJoinError as u8 => Ok(ServerToClientMessageTypePrimitive::ConferenceJoinError),
            x if x == ServerToClientMessageTypePrimitive::ConferenceLeaveError as u8 => Ok(ServerToClientMessageTypePrimitive::ConferenceLeaveError),
            x if x == ServerToClientMessageTypePrimitive::MessageError as u8 => Ok(ServerToClientMessageTypePrimitive::MessageError),
            _ => Err(()),
        }
    }
}

pub type MessageID = usize;

pub enum UIAction {
    /// Create a new conference with the given password.
    CreateConference(String),
    /// Join a conference with the given ID and password.
    JoinConference((ConferenceId, String)),
    /// Leave a conference with the given ID.
    LeaveConference(ConferenceId),
    /// Send a message to a conference.
    SendMessage((ConferenceId, MessageID, String)),
    /// Disconnect from the server.
    Disconnect,
}

pub enum UIEvent {
    ConferenceCreated(ConferenceId),
    ConferenceCreateFailed,
    ConferenceJoined((ConferenceId, NumberOfPeers)),
    ConferenceJoinFailed(ConferenceId),
    ConferenceLeft(ConferenceId),
    ConferenceLeaveFailed(ConferenceId),
    IncomingMessage((ConferenceId, Vec<u8>, bool)),
    MessageAccepted((ConferenceId, MessageID)),
    MessageRejected((ConferenceId, MessageID)),
    MessageError((ConferenceId, MessageID)),
    ConferenceRestructuring((ConferenceId, NumberOfPeers)),
    ConferenceRestructuringFinished(ConferenceId),
}

pub const SERVER_NAME: &str = "anonymous-conference.program";

pub const PROTOCOL_HEADER: &[u8] = b"\x1CAnonymousConference protocol";


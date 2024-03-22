use std::{collections::HashSet, hash::Hash};

use crate::{constants::{
    Receiver,
    Sender,
    Result,
    ServerEvent,
    UIEvent,
    ConferenceId,
    NumberOfPeers,
    EncryptionKey,
    Message,
}, crypto::KEY_SIZE};

use futures::prelude::*;

use log::{debug, warn};
use openssl::{rsa::Rsa, pkey::{Public, Private}};
use crate::crypto;

enum ConferenceState {
    Initial,
    PublicKeyExchange,
    PublicKeyExchangeFinished,
    EncryptionKeyNegotiation,
    EncryptionKeyNegotiationFinished,
    NormalOperation,
}

const RSA_KEY_SIZE: u32 = 2048;

struct PublicKey {
    key: Rsa<Public>,
}

impl PartialEq for PublicKey {
    fn eq(&self, other: &Self) -> bool {
        self.key.n().eq(other.key.n()) && self.key.e().eq(other.key.e())
    }
}

impl Eq for PublicKey {}

impl Hash for PublicKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key.n().to_hex_str().unwrap().hash(state);
        self.key.e().to_hex_str().unwrap().hash(state);
    }
}

#[repr(u8)]
/// The different types of messages that can be sent between clients
/// PublicKey = `0x01`
/// EncryptionKeyPart = `0x02`
/// Message = `0x03`
enum ClientToClientMessage {
    PublicKey(Vec<u8>),
    EncryptionKeyPart(Vec<u8>),
    Message(Vec<u8>),
}

impl ClientToClientMessage {
    fn encode(&self) -> Vec<u8> {
        match self {
            ClientToClientMessage::PublicKey(pubkey) => {
                let mut result = Vec::new();
                result.push(0x01);
                result.extend_from_slice(pubkey);
                result
            },
            ClientToClientMessage::EncryptionKeyPart(key_part) => {
                let mut result = Vec::new();
                result.push(0x02);
                result.extend_from_slice(key_part);
                result
            },
            ClientToClientMessage::Message(message) => {
                let mut result = Vec::new();
                result.push(0x03);
                result.extend_from_slice(&message.len().to_be_bytes());
                result.extend_from_slice(message);
                result
            },
        }
    }
}

pub struct ConferenceManager {
    conference_id: ConferenceId,
    number_of_peers: NumberOfPeers,
    initial_encryption_key: EncryptionKey,
    conference_event_receiver: Receiver<ServerEvent>,
    message_sender: Sender<Message>,
    ui_event_sender: Sender<UIEvent>,
    public_keys: HashSet<PublicKey>,
    private_key: Rsa<Private>,
    state: ConferenceState,
    ephemeral_key_parts: NumberOfPeers,
    new_ephemeral_key: EncryptionKey,
    ephemeral_encryption_key: Option<EncryptionKey>,
}

impl ConferenceManager {
    pub fn new(
        conference_id: ConferenceId,
        number_of_peers: NumberOfPeers,
        initial_encryption_key: EncryptionKey,
        conference_event_receiver: Receiver<ServerEvent>,
        message_sender: Sender<Message>,
        ui_event_sender: Sender<UIEvent>,
    ) -> ConferenceManager {
        debug!("Generating RSA key for conference {}", conference_id);
        let private_key = Rsa::generate(RSA_KEY_SIZE).expect("Could not generate RSA key");
        debug!("Generated RSA key for conference {}", conference_id);
        ConferenceManager {
            conference_id,
            number_of_peers,
            initial_encryption_key,
            conference_event_receiver,
            message_sender,
            ui_event_sender,
            public_keys: HashSet::with_capacity(number_of_peers as usize - 1),
            private_key,
            state: ConferenceState::Initial,
            ephemeral_key_parts: 0,
            new_ephemeral_key: [0; 32], // temp value
            ephemeral_encryption_key: None,
        }
    }

    pub async fn start_conference_manager(&mut self) -> Result<()> {
        debug!("Starting conference manager for conference {}", self.conference_id);

        // start initial public key exchange
        self.start_public_key_exchange().await;

        while let Some(server_event) = self.conference_event_receiver.next().await {
            match server_event {
                ServerEvent::ConferenceRestructuring((_, number_of_peers)) => self.initiate_conference_restructuring(number_of_peers).await,
                ServerEvent::IncomingMessage((_, message)) => self.process_incoming_message(message).await,
                _ => panic!("ConferenceManager received unexpected event")
            }
        }

        debug!("Conference manager for conference {} has stopped", self.conference_id);
        Ok(())
    }

    async fn initiate_conference_restructuring(&mut self, new_number_of_peers: NumberOfPeers) {
        debug!("Conference {} is being restructured to {} peers", self.conference_id, new_number_of_peers);
        self.number_of_peers = new_number_of_peers;
        self.public_keys.clear();
        debug!("Generating own part of the new ephemeral key for conference {}", self.conference_id);
        self.new_ephemeral_key = crypto::generate_ephemeral_key();
        self.ephemeral_key_parts = 0;
        self.start_public_key_exchange().await;
    }

    async fn start_public_key_exchange(&mut self) {
        debug!("Starting initial public key exchange for conference {}", self.conference_id);
        self.state = ConferenceState::PublicKeyExchange;
        self.send_message(ClientToClientMessage::PublicKey(self.private_key.public_key_to_der_pkcs1().unwrap())).await;
    }

    async fn start_ephemeral_key_negotiation(&mut self) {
        debug!("Starting ephemeral encryption key negotiation for conference {}", self.conference_id);
        self.state = ConferenceState::EncryptionKeyNegotiation;
    }

    async fn process_incoming_message(&mut self, message: Vec<u8>) {
        debug!("Received message for conference {}", self.conference_id);
        match self.state {
            ConferenceState::Initial => {
                // ignore message
                warn!("Received message for conference {} in initial state, ignoring", self.conference_id);
            },
            ConferenceState::PublicKeyExchange => self.process_message_public_key_exchange(message).await,
            ConferenceState::EncryptionKeyNegotiation => self.process_message_ephemeral_key_negotiation(message).await,
            ConferenceState::NormalOperation => self.process_message_normal_operation(message).await,
            _ => {
                // very unlikely to happen
                warn!("Received message for conference {} in unexpected state, ignoring", self.conference_id);
            }
        }
    }

    async fn process_message_public_key_exchange(&mut self, message: Vec<u8>) {
        if let Some(message) = self.read_message(message).await {
            match message {
                ClientToClientMessage::PublicKey(pubkey) => {
                    if let Ok(pubkey) = Rsa::public_key_from_der_pkcs1(&pubkey) {
                        self.public_keys.insert(PublicKey{key: pubkey});
                        debug!("Received public key from peer in conference {}, now have {} public keys", self.conference_id, self.public_keys.len());
                        if self.public_keys.len() == self.number_of_peers as usize - 1 {
                            debug!("Received all public keys for conference {}", self.conference_id);
                            self.state = ConferenceState::PublicKeyExchangeFinished;
                            self.start_ephemeral_key_negotiation().await;
                        }
                    } else {
                        warn!("Received invalid public key from peer for conference {}", self.conference_id);
                    }
                },
                ClientToClientMessage::Message(message) => {
                    // the message was decrypted with old encryption key
                    debug!("Received text message from peer for conference {} while in public key exchange state", self.conference_id);
                    self.process_text_message(message).await;
                },
                _ => {
                    warn!("Received unexpected message from peer for conference {} while in public key exchange state", self.conference_id);
                }
            }
        } else {
            warn!("Received invalid message from peer for conference {} while in public key exchange state", self.conference_id);
        }
    }

    async fn process_message_ephemeral_key_negotiation(&mut self, message: Vec<u8>) {
        if let Some(message) = self.read_message(message).await {
            match message {
                ClientToClientMessage::EncryptionKeyPart(key_part) => {
                    if key_part.len() != KEY_SIZE {
                        warn!("Received invalid encryption key part from peer for conference {}, key part too short", self.conference_id);
                        return;
                    }
                    crypto::apply_ephemeral_key_part(&mut self.new_ephemeral_key, &key_part);
                    self.ephemeral_key_parts += 1;
                    debug!("Received {} of {} encryption key parts for conference {}", self.ephemeral_key_parts, self.number_of_peers - 1, self.conference_id);
                    if self.ephemeral_key_parts == self.number_of_peers - 1 {
                        debug!("Received all encryption key parts for conference {}", self.conference_id);
                        self.ephemeral_encryption_key = Some(self.new_ephemeral_key);
                        self.state = ConferenceState::EncryptionKeyNegotiationFinished;
                        self.finish_conference_setup().await;
                    }
                },
                ClientToClientMessage::Message(message) => {
                    // the message was decrypted with old encryption key
                    debug!("Received text message from peer for conference {} while in encryption key negotiation state", self.conference_id);
                    self.process_text_message(message).await;
                },
                _ => {
                    warn!("Received unexpected message from peer for conference {} while in encryption key negotiation state", self.conference_id);
                },
            }
        } else {
            warn!("Received invalid message from peer for conference {} while in encryption key negotiation state", self.conference_id);
        }
    }

    async fn finish_conference_setup(&mut self) {
        debug!("Conference {} setup finished", self.conference_id);
        self.state = ConferenceState::NormalOperation;
    }

    async fn process_message_normal_operation(&mut self, message: Vec<u8>) {
        if let Some(message) = self.read_message(message).await {
            match message {
                ClientToClientMessage::Message(message) => {
                    debug!("Received text message from peer for conference {}", self.conference_id);
                    self.process_text_message(message).await;
                },
                _ => {
                    warn!("Received unexpected message from peer for conference {}", self.conference_id);
                },
            }
        } else {
            warn!("Received invalid message from peer for conference {}", self.conference_id);
        }
    }

    /// Send a message to the conference, returns `true` if the message was sent successfully
    async fn send_message(&mut self, message: ClientToClientMessage) -> bool {
        match message {
            ClientToClientMessage::PublicKey(_) | ClientToClientMessage::EncryptionKeyPart(_) => {
                let encrypted_message = crypto::encrypt_message(&message.encode(), &self.initial_encryption_key).unwrap();
                self.message_sender.send(
                    Message{conference: self.conference_id, message: encrypted_message.encode()}
                ).await.expect("Could not send message");
            },
            ClientToClientMessage::Message(_) => {
                if let Some(ephemeral_encryption_key) = self.ephemeral_encryption_key {
                    let signed_message = self.sing_message(message.encode()).await;
                    let encrypted_message = crypto::encrypt_message(&signed_message, &ephemeral_encryption_key).unwrap();
                    self.message_sender.send(
                        Message{conference: self.conference_id, message: encrypted_message.encode()}
                    ).await.expect("Could not send message");
                } else {
                    return false;
                }
            },
        }
        true
    }

    async fn sing_message(&mut self, message: Vec<u8>) -> Vec<u8> {
        todo!();
    }

    async fn check_message_signature(&mut self, message: Vec<u8>) -> bool {
        todo!();
    }

    async fn read_message(&mut self, message: Vec<u8>) -> Option<ClientToClientMessage> {
        assert!(!message.is_empty());
        match message[0] {
            0x01 => {
                // PublicKey
                Some(ClientToClientMessage::PublicKey(message[1..].to_vec()))
            },
            0x02 => {
                // EncryptionKeyPart
                Some(ClientToClientMessage::EncryptionKeyPart(message[1..].to_vec()))
            },
            0x03 => {
                // Message
                if message.len() < 5 {
                    warn!("Received text message with invalid length from peer for conference {} (not enought bytes to read message length)", self.conference_id);
                    return None;
                }
                let message_length = u32::from_be_bytes(message[1..5].try_into().unwrap());
                if message.len() != 5 + message_length as usize {
                    warn!("Received text message with invalid length from peer for conference {} (message length is incorrect)", self.conference_id);
                    return None;
                }
                Some(ClientToClientMessage::Message(message[5..].to_vec()))
            },
            _ => {
                warn!("Received message with invalid message type 0x{} from peer for conference {}", message[0], self.conference_id);
                None
            }

        }
    }

    async fn process_text_message(&mut self, message: Vec<u8>) {
        todo!();
    }
}

#[cfg(test)]
mod tests {
    use async_std::task;
    use futures::channel::mpsc;

    use super::*;

    #[test]
    fn test_start_conference_manager() {
        let (_, conference_event_receiver) = mpsc::unbounded();
        let (message_sender, _) = mpsc::unbounded();
        let (ui_event_sender, _) = mpsc::unbounded();
        let mut conference_manager = ConferenceManager::new( 0, 1, [0; 32], conference_event_receiver, message_sender, ui_event_sender);

        task::block_on(async move {conference_manager.start_conference_manager().await.unwrap()});
    }
}

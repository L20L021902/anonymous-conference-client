use std::collections::HashSet;

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

use curve25519_dalek::{Scalar, RistrettoPoint, ristretto::CompressedRistretto, constants::RISTRETTO_BASEPOINT_POINT};
use futures::prelude::*;

use log::{debug, warn, info};
use crate::crypto;

enum ConferenceState {
    Initial,
    PublicKeyExchange,
    PublicKeyExchangeFinished,
    EncryptionKeyNegotiation,
    EncryptionKeyNegotiationFinished,
    NormalOperation,
}

#[repr(u8)]
/// The different types of messages that can be sent between clients
/// PublicKey = `0x01`
/// EncryptionKeyPart = `0x02`
/// Message = `0x03`
enum ClientToClientMessage {
    PublicKey([u8; 32]),
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
    _unsorted_public_keys: HashSet<CompressedRistretto>,
    ring: Option<Vec<RistrettoPoint>>,
    ring_personal_key_index: Option<usize>,
    personal_private_key: Scalar,
    personal_public_key: RistrettoPoint,
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
        debug!("Generating personal key pair for conference {}", conference_id);
        let mut csprng = rand_core::OsRng;
        let personal_private_key = Scalar::random(&mut csprng);
        let personal_public_key = personal_private_key * RISTRETTO_BASEPOINT_POINT;

        let mut _unsorted_public_keys = HashSet::with_capacity(number_of_peers as usize); // including the personal key
        _unsorted_public_keys.insert(personal_public_key.compress());
        debug!("Generated personal key pair for conference {}", conference_id);

        ConferenceManager {
            conference_id,
            number_of_peers,
            initial_encryption_key,
            conference_event_receiver,
            message_sender,
            ui_event_sender,
            _unsorted_public_keys,
            ring: None,
            ring_personal_key_index: None,
            personal_private_key,
            personal_public_key,
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
        self._unsorted_public_keys.clear();
        self._unsorted_public_keys.insert(self.personal_public_key.compress());
        // not resetting the self.ring yet because we might receive old messages while restructuring
        debug!("Generating own part of the new ephemeral key for conference {}", self.conference_id);
        self.new_ephemeral_key = crypto::generate_ephemeral_key();
        self.ephemeral_key_parts = 0;
        self.start_public_key_exchange().await;
    }

    async fn start_public_key_exchange(&mut self) {
        debug!("Starting initial public key exchange for conference {}", self.conference_id);
        self.state = ConferenceState::PublicKeyExchange;
        self.send_message(ClientToClientMessage::PublicKey(*self.personal_public_key.compress().as_bytes())).await;
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
                    let compressed = CompressedRistretto::from_slice(&pubkey).unwrap(); // should never fail since PublicKey has to be [u8; 32]
                    self._unsorted_public_keys.insert(compressed);
                    debug!("Received public key from peer in conference {}, now have {} public keys", self.conference_id, self._unsorted_public_keys.len());
                    if self._unsorted_public_keys.len() == self.number_of_peers as usize {
                        debug!("Received all public keys for conference {}", self.conference_id);
                        self.finish_public_key_exchange().await;
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

    async fn finish_public_key_exchange(&mut self) {
        self.state = ConferenceState::PublicKeyExchangeFinished;

        let mut compressed_ring: Vec<CompressedRistretto> = self._unsorted_public_keys.iter().cloned().collect();
        compressed_ring.sort_unstable(); // sort the keys in order

        self.ring_personal_key_index = Some(compressed_ring.iter().position(|key| key == &self.personal_public_key.compress()).unwrap());
        
        self.ring = Some(compressed_ring.iter().map(|key| key.decompress().unwrap()).collect());

        self.start_ephemeral_key_negotiation().await;
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
                    if self.ring.is_none() || self.ring_personal_key_index.is_none() {
                        warn!("Tried to send message for conference {} while not fully set up", self.conference_id);
                        return false;
                    }
                    let signed_message = self.sign_message(message.encode()).await;
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

    /// Sign a message with the ring signature
    /// returns the signature + message
    async fn sign_message(&self, message: Vec<u8>) -> Vec<u8> {
        assert!(self.ring.is_some());
        assert!(self.ring_personal_key_index.is_some());
        let signature = crypto::sign_message(&self.personal_private_key, self.ring_personal_key_index.unwrap(), self.ring.as_ref().unwrap(), &message);
        let mut result = Vec::with_capacity(32 + 32 * self.number_of_peers as usize + 32 + message.len());
        result.extend_from_slice(&signature.challenge.to_bytes());
        for response in signature.responses.iter() {
            result.extend_from_slice(&response.to_bytes());
        }
        result.extend_from_slice(&signature.key_image.compress().to_bytes());
        result.extend_from_slice(&message);
        result
    }

    /// Check the signature of a signed message
    /// returns the message and `true` if the signature is valid
    async fn check_message_signature(&mut self, message: Vec<u8>) -> Option<(Vec<u8>, bool)> {
        if message.len() < 32 + 32 * self.number_of_peers as usize + 32 {
            warn!("Received signed message with invalid length from peer for conference {} (not enough bytes to read signature)", self.conference_id);
            return None;
        }

        // parse signature
        let challenge = Scalar::from_canonical_bytes(message[0..32].try_into().unwrap());
        if challenge.is_none().into() {
            warn!("Received signed message with invalid signature from peer for conference {} (could not parse challenge)", self.conference_id);
            return None;
        }
        let challenge = challenge.unwrap();
        let mut responses = Vec::with_capacity(self.number_of_peers as usize);
        for response in message[32..32 + 32 * self.number_of_peers as usize].chunks_exact(32) {
            let response = Scalar::from_canonical_bytes(response.try_into().unwrap());
            if response.is_none().into() {
                warn!("Received signed message with invalid signature from peer for conference {} (could not parse response)", self.conference_id);
                return None;
            }
            responses.push(response.unwrap());
        }
        let key_image = CompressedRistretto::from_slice(&message[32 + 32 * self.number_of_peers as usize..32 + 32 * self.number_of_peers as usize + 32]);
        if key_image.is_ok() {
            warn!("Received signed message with invalid signature from peer for conference {} (could not parse key image)", self.conference_id);
            return None;
        }
        let key_image = key_image.unwrap();

        let signature = crypto::BLSAG_COMPACT {
            challenge,
            responses,
            key_image: key_image.decompress().unwrap(),
        };

        let message = message[32 + 32 * self.number_of_peers as usize + 32..].to_vec();
        let signature_valid = crypto::verify_message(&signature, self.ring.as_ref().unwrap(), &message);

        Some((message, signature_valid))
    }

    async fn read_message(&mut self, message: Vec<u8>) -> Option<ClientToClientMessage> {
        assert!(!message.is_empty());
        match message[0] {
            0x01 => {
                // PublicKey
                if message.len() != 33 {
                    warn!("Received public key message with invalid length from peer for conference {} (expected 33 bytes, got {})", self.conference_id, message.len());
                    return None;
                }
                Some(ClientToClientMessage::PublicKey(message[1..].try_into().unwrap()))
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
        let Some((message, is_signature_valid)) = self.check_message_signature(message).await
        else {
            warn!("Received invalid signed message from peer for conference {}", self.conference_id);
            return;
        };
        info!("Received message from peer for conference {}", self.conference_id);
        // TODO notify the UI
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

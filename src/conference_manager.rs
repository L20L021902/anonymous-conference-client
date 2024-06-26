use std::collections::HashSet;

use crate::{constants::{
    Receiver,
    Sender,
    Result,
    UIEvent,
    ConferenceId,
    NumberOfPeers,
    EncryptionKey,
    Message, ConferenceEvent,
}, crypto::KEY_SIZE};

use async_std::stream::StreamExt;
use async_std::io::{Cursor, ReadExt};
use curve25519_dalek::{Scalar, RistrettoPoint, ristretto::CompressedRistretto, constants::RISTRETTO_BASEPOINT_POINT};
use futures::SinkExt;

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
                result.extend_from_slice(&u32::try_from(message.len()).unwrap().to_be_bytes());
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
    conference_event_receiver: Receiver<ConferenceEvent>,
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
        conference_event_receiver: Receiver<ConferenceEvent>,
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
                ConferenceEvent::ConferenceRestructuring(number_of_peers) => self.initiate_conference_restructuring(number_of_peers).await,
                ConferenceEvent::IncomingMessage(message) => self.process_incoming_message(message).await,
                ConferenceEvent::OutboundMessage((message_id, message)) => self.process_outbound_message(message_id, message).await,
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
        self.send_message(ClientToClientMessage::PublicKey(*self.personal_public_key.compress().as_bytes()), None).await;
    }

    async fn start_ephemeral_key_negotiation(&mut self) {
        debug!("Starting ephemeral encryption key negotiation for conference {}", self.conference_id);
        self.state = ConferenceState::EncryptionKeyNegotiation;
        self.send_message(ClientToClientMessage::EncryptionKeyPart(self.new_ephemeral_key.to_vec()), None).await;
    }

    async fn process_incoming_message(&mut self, message: Vec<u8>) {
        debug!("Received message for conference {}, len is {}", self.conference_id, message.len());
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

    async fn process_outbound_message(&mut self, message_id: usize, message: Vec<u8>) {
        match self.state {
            ConferenceState::NormalOperation => {
                assert!(self.ring.is_some() && self.ring_personal_key_index.is_some() && self.ephemeral_encryption_key.is_some());
                // sign message
                let signed_message = self.sign_message(message).await;
                // send message
                self.send_message(ClientToClientMessage::Message(signed_message), Some(message_id)).await;
            }
            _ => {
                warn!("Tried to send message for conference {} while not fully set up", self.conference_id);
                self.ui_event_sender.send(UIEvent::MessageError((self.conference_id, message_id))).await.unwrap();
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
        self.ui_event_sender.send(UIEvent::ConferenceRestructuringFinished(self.conference_id)).await.unwrap();
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

    /// Send a message to the conference
    async fn send_message(&mut self, message: ClientToClientMessage, message_id: Option<usize>) {
        match message {
            ClientToClientMessage::PublicKey(_) | ClientToClientMessage::EncryptionKeyPart(_) => {
                let encrypted_message = crypto::encrypt_message(&message.encode(), &self.initial_encryption_key).unwrap();
                self.message_sender.send(
                    Message{conference: self.conference_id, message: encrypted_message.encode(), message_id: None}
                ).await.expect("Could not send message");
            },
            ClientToClientMessage::Message(_) => {
                assert!(self.ephemeral_encryption_key.is_some());
                assert!(message_id.is_some());
                let encrypted_message = crypto::encrypt_message(&message.encode(), &self.ephemeral_encryption_key.unwrap()).unwrap();
                self.message_sender.send(
                    Message{conference: self.conference_id, message: encrypted_message.encode(), message_id}
                ).await.unwrap();
            },
        }
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

        const SCALAR_BYTE_SIZE: usize = 32;
        let mut message_reader = Cursor::new(message);
        let mut buffer = [0; SCALAR_BYTE_SIZE];

        // parse signature
        if message_reader.read_exact(&mut buffer).await.is_err() {
            warn!("Received signed message with invalid signature from peer for conference {} (could not read challenge)", self.conference_id);
            return None;
        }
        let Some(challenge) = Scalar::from_canonical_bytes(buffer).into()
        else {
            warn!("Received signed message with invalid signature from peer for conference {} (could not parse challenge)", self.conference_id);
            return None;
        };

        let mut responses = Vec::with_capacity(self.number_of_peers as usize);
        for _ in 0..self.number_of_peers {
            if message_reader.read_exact(&mut buffer).await.is_err() {
                warn!("Received signed message with invalid signature from peer for conference {} (could not read response)", self.conference_id);
                return None;
            }
            if let Some(response) = Scalar::from_canonical_bytes(buffer).into() {
                responses.push(response);
            } else {
                warn!("Received signed message with invalid signature from peer for conference {} (could not parse response)", self.conference_id);
                return None;
            }
        }

        if message_reader.read_exact(&mut buffer).await.is_err() {
            warn!("Received signed message with invalid signature from peer for conference {} (could not read key image)", self.conference_id);
            return None;
        }
        let Ok(key_image) = CompressedRistretto::from_slice(&buffer)
        else {
            warn!("Received signed message with invalid signature from peer for conference {} (could not parse key image)", self.conference_id);
            return None;
        };
        let Some(key_image) = key_image.decompress()
        else {
            warn!("Received signed message with invalid signature from peer for conference {} (could not decompress key image)", self.conference_id);
            return None;
        };

        let signature = crypto::BLSAG_COMPACT {
            challenge,
            responses,
            key_image,
        };

        let mut message = Vec::new();
        if message_reader.read_to_end(&mut message).await.is_err() {
            warn!("Received signed message with invalid signature from peer for conference {} (could not read message)", self.conference_id);
            return None;
        }
        let signature_valid = crypto::verify_message(&signature, self.ring.as_ref().unwrap(), &message);

        Some((message, signature_valid))
    }

    async fn decrypt_message_helper(&self, message: Vec<u8>) -> Option<Vec<u8>> {
        if let Ok(encrypted_message) = crypto::EncryptionResult::decode(&message) {
            if let Some(ephemeral_encryption_key) = self.ephemeral_encryption_key {
                // could either be encrypted using the ephemeral key or the initial key
                match self.state {
                    ConferenceState::NormalOperation => {
                        // first try ephemeral_encryption_key, then initial_encryption_key
                        if let Ok(decrypted_message) = crypto::decrypt_message(&ephemeral_encryption_key, &encrypted_message) {
                            debug!("Decrypted message using ephemeral_encryption_key in conference {}", self.conference_id);
                            return Some(decrypted_message);
                        } else if let Ok(decrypted_message) = crypto::decrypt_message(&self.initial_encryption_key, &encrypted_message) {
                            debug!("Decrypted message using initial_encryption_key in conference {}", self.conference_id);
                            return Some(decrypted_message);
                        } else {
                            warn!("Received invalid message from peer for conference {} (could not decrypt message)", self.conference_id);
                            return None;
                        }
                    },
                    _ => {
                        // first try initial_encryption_key, then ephemeral_encryption_key (probably old)
                        if let Ok(decrypted_message) = crypto::decrypt_message(&self.initial_encryption_key, &encrypted_message) {
                            debug!("Decrypted message using initial_encryption_key in conference {}", self.conference_id);
                            return Some(decrypted_message);
                        } else if let Ok(decrypted_message) = crypto::decrypt_message(&ephemeral_encryption_key, &encrypted_message) {
                            debug!("Decrypted message using ephemeral_encryption_key in conference {}", self.conference_id);
                            return Some(decrypted_message);
                        } else {
                            warn!("Received invalid message from peer for conference {} (could not decrypt message)", self.conference_id);
                            return None;
                        }
                    },
                }
            } else {
                // only initial_encryption_key is available
                if let Ok(decrypted_message) = crypto::decrypt_message(&self.initial_encryption_key, &encrypted_message) {
                    debug!("Decrypted message from peer for conference {} using initial_encryption_key", self.conference_id);
                    return Some(decrypted_message);
                } else {
                    warn!("Received invalid message from peer for conference {} (could not decrypt message)", self.conference_id);
                    return None;
                }
            }
        } else {
            warn!("Received invalid message from peer for conference {} (could not decode encrypted message)", self.conference_id);
            return None;
        }
    }

    async fn read_message(&mut self, message: Vec<u8>) -> Option<ClientToClientMessage> {
        assert!(!message.is_empty());
        let Some(message) = self.decrypt_message_helper(message).await
        else {
            warn!("Received invalid message from peer for conference {}", self.conference_id);
            return None;
        };

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
                warn!("Received message with invalid message type {} from peer for conference {}", message[0], self.conference_id);
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
        self.ui_event_sender.send(UIEvent::IncomingMessage((self.conference_id, message, is_signature_valid))).await.unwrap();
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

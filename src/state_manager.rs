use std::collections::HashMap;

use async_std::{prelude::*, task};
use futures::{channel::mpsc, select, FutureExt, SinkExt, AsyncReadExt, AsyncWriteExt};
use log::{error, info, warn};
use crate::{
    connection_manager,
    conference_manager,
    constants::{
        ClientEvent, ConferenceEvent, ConferenceId, Message, MessageID, NumberOfPeers, PacketNonce, Receiver, Sender, ServerEvent, UIAction, UIEvent
    },
    crypto,
};

#[derive(PartialEq, Eq, Debug)]
enum SentEvent {
    CreateConference,
    GetConferenceJoinSalt((ConferenceId, String)),
    JoinConference((ConferenceId, String)),
    LeaveConference(ConferenceId),
    SendMessage((ConferenceId, MessageID)),
    Disconnect,
}

async fn start_state_manager(server_address: String, mut ui_event_sender: Sender<UIEvent>, mut ui_action_receiver: Receiver<UIAction>) {
    let (server_event_sender, mut server_event_receiver) = mpsc::unbounded();
    let (mut client_event_sender, client_event_receiver) = mpsc::unbounded();
    let (message_sender, mut message_receiver) = mpsc::unbounded::<Message>();

    // start connection_manager
    task::spawn(async move {connection_manager::start_connection_manager(server_address, server_event_sender, client_event_receiver).await});

    let mut conferences: HashMap<ConferenceId, Sender<ConferenceEvent>> = HashMap::new();
    let mut send_packets_last_index: PacketNonce = 0;
    let mut sent_packets: HashMap<PacketNonce, SentEvent> = HashMap::new();


    loop {
        select! {
            server_event = server_event_receiver.next().fuse() => match server_event {
                // handle server events
                Some(server_event) => {
                    match server_event {
                        ServerEvent::HandshakeAcknowledged => {
                            panic!("This shouldn't happen");
                        },
                        ServerEvent::ConferenceCreated((packet_nonce, conference_id)) => {
                            if let Some(sent_event) = sent_packets.get(&packet_nonce) {
                                if let SentEvent::CreateConference = sent_event {
                                    ui_event_sender.send(UIEvent::ConferenceCreated(conference_id)).await.unwrap();
                                    sent_packets.remove(&packet_nonce);
                                } else {
                                    warn!("Received unexpected packet with nonce {} from CreateConference event, instead got {:?}", packet_nonce, sent_event);
                                }
                            } else {
                                warn!("Received unexpected CreateConference packet with nonce {}", packet_nonce);
                            }
                        },
                        ServerEvent::ConferenceJoinSalt((packet_nonce, conference_id, join_salt)) => {
                            if let Some(sent_event) = sent_packets.get(&packet_nonce) {
                                if let SentEvent::GetConferenceJoinSalt((expected_conference_id, password)) = sent_event {
                                    if conference_id != *expected_conference_id {
                                        warn!("Received unexpected conference id {} from GetConferenceJoinSalt event, instead got {}", conference_id, expected_conference_id);
                                        continue;
                                    }
                                    send_packets_last_index += 1;
                                    let new_packet_nonce = send_packets_last_index;
                                    let password_hash = crypto::hash_password_with_salt(password.as_bytes(), &join_salt);
                                    let packet = ClientEvent::JoinConference((new_packet_nonce, conference_id, password_hash));
                                    let password_clone = password.clone();
                                    sent_packets.remove(&packet_nonce);
                                    sent_packets.insert(new_packet_nonce, SentEvent::JoinConference((conference_id, password_clone)));
                                    client_event_sender.send(packet).await.unwrap();
                                } else {
                                    warn!("Received unexpected packet with nonce {} from GetConferenceJoinSalt event, instead got {:?}", packet_nonce, sent_event);
                                }
                            } else {
                                warn!("Received unexpected ConferenceJoinSalt packet with nonce {}", packet_nonce);
                            }
                        },
                        ServerEvent::ConferenceJoined((packet_nonce, conference_id, number_of_peers, encryption_salt)) => {
                            if let Some(sent_event) = sent_packets.get(&packet_nonce) {
                                if let SentEvent::JoinConference((expected_conference_id, password)) = sent_event {
                                    if conference_id != *expected_conference_id {
                                        warn!("Received unexpected conference id {} from JoinConference event, instead got {}", conference_id, expected_conference_id);
                                        continue;
                                    }
                                    let password_clone = password.clone();
                                    sent_packets.remove(&packet_nonce);
                                    conferences.insert(conference_id,
                                        create_conference(
                                            conference_id, number_of_peers, password_clone.as_bytes(), 
                                            &encryption_salt, message_sender.clone(), ui_event_sender.clone()
                                    ).await);
                                    ui_event_sender.send(UIEvent::ConferenceJoined((conference_id, number_of_peers))).await.unwrap();
                                } else {
                                    warn!("Received unexpected packet with nonce {} from CreateConference event, instead got {:?}", packet_nonce, sent_event);
                                }
                            } else {
                                warn!("Received unexpected CreateConference packet with nonce {}", packet_nonce);
                            }
                        },
                        ServerEvent::ConferenceLeft((packet_nonce, conference_id)) => {
                            if let Some(sent_event) = sent_packets.get(&packet_nonce) {
                                if let SentEvent::LeaveConference(expected_conference_id) = sent_event {
                                    if conference_id != *expected_conference_id {
                                        warn!("Received unexpected conference id {} from LeaveConference event, instead got {}", conference_id, expected_conference_id);
                                        continue;
                                    }
                                    ui_event_sender.send(UIEvent::ConferenceLeft(conference_id)).await.unwrap();
                                    sent_packets.remove(&packet_nonce);
                                    conferences.remove(&conference_id);
                                } else {
                                    warn!("Received unexpected packet with nonce {} from LeaveConference event, instead got {:?}", packet_nonce, sent_event);
                                }
                            } else {
                                warn!("Received unexpected LeaveConference packet with nonce {}", packet_nonce);
                            }
                        },
                        ServerEvent::MessageAccepted((packet_nonce, conference_id)) => {
                            if let Some(sent_event) = sent_packets.get(&packet_nonce) {
                                if let SentEvent::SendMessage((expected_conference_id, message_id)) = sent_event {
                                    if conference_id != *expected_conference_id {
                                        warn!("Received unexpected conference id {} from SendMessage event, instead got {}", conference_id, expected_conference_id);
                                        continue;
                                    }
                                    ui_event_sender.send(UIEvent::MessageAccepted((conference_id, *message_id))).await.unwrap();
                                    sent_packets.remove(&packet_nonce);
                                } else {
                                    warn!("Received unexpected packet with nonce {} from SendMessage event, instead got {:?}", packet_nonce, sent_event);
                                }
                            } else {
                                warn!("Received unexpected SendMessage packet with nonce {}", packet_nonce);
                            }
                        },
                        ServerEvent::ConferenceRestructuring((conference_id, number_of_peers)) => {
                            if let Some(mut conference_sender) = conferences.get(&conference_id) {
                                conference_sender.send(ConferenceEvent::ConferenceRestructuring(number_of_peers)).await.unwrap();
                                ui_event_sender.send(UIEvent::ConferenceRestructuring((conference_id, number_of_peers))).await.unwrap();
                            } else {
                                warn!("Attempted to restructure non-existent conference {}", conference_id);
                            }
                        },
                        ServerEvent::IncomingMessage((conference_id, message)) => {
                            if let Some(mut conference_sender) = conferences.get(&conference_id) {
                                conference_sender.send(ConferenceEvent::IncomingMessage(message)).await.unwrap();
                            } else {
                                warn!("Received a message for a non-existent conference {}", conference_id);
                            }
                        },
                        ServerEvent::GeneralError => {
                            error!("Received a general error from the server");
                            break;
                        },
                        ServerEvent::ConferenceCreationError(packet_nonce) => {
                            if let Some(sent_event) = sent_packets.get(&packet_nonce) {
                                if let SentEvent::CreateConference = sent_event {
                                    ui_event_sender.send(UIEvent::ConferenceCreateFailed).await.unwrap();
                                    sent_packets.remove(&packet_nonce);
                                } else {
                                    warn!("Received unexpected packet with nonce {} from CreateConference event, instead got {:?}", packet_nonce, sent_event);
                                }
                            } else {
                                warn!("Received unexpected ConferenceCreationError packet with nonce {}", packet_nonce);
                            }
                        },
                        ServerEvent::ConferenceJoinSaltError((packet_nonce, conference_id)) => {
                            if let Some(sent_event) = sent_packets.get(&packet_nonce) {
                                if let SentEvent::GetConferenceJoinSalt((expected_conference_id, _)) = sent_event {
                                    if conference_id != *expected_conference_id {
                                        warn!("Received unexpected conference id {} from ConferenceJoinSaltError event, instead got {}", conference_id, expected_conference_id);
                                        continue;
                                    }
                                    ui_event_sender.send(UIEvent::ConferenceJoinFailed(conference_id)).await.unwrap();
                                    sent_packets.remove(&packet_nonce);
                                } else {
                                    warn!("Received unexpected packet with nonce {} from ConferenceJoinSaltError event, instead got {:?}", packet_nonce, sent_event);
                                }
                            } else {
                                warn!("Received unexpected ConferenceJoinSaltError packet with nonce {}", packet_nonce);
                            }
                        },
                        ServerEvent::ConferenceJoinError((packet_nonce, conference_id)) => {
                            if let Some(sent_event) = sent_packets.get(&packet_nonce) {
                                if let SentEvent::JoinConference((expected_conference_id, _)) = sent_event {
                                    if conference_id != *expected_conference_id {
                                        warn!("Received unexpected conference id {} from ConferenceJoinError event, instead got {}", conference_id, expected_conference_id);
                                        continue;
                                    }
                                    ui_event_sender.send(UIEvent::ConferenceJoinFailed(conference_id)).await.unwrap();
                                    sent_packets.remove(&packet_nonce);
                                } else {
                                    warn!("Received unexpected packet with nonce {} from ConferenceJoinError event, instead got {:?}", packet_nonce, sent_event);
                                }
                            } else {
                                warn!("Received unexpected ConferenceJoinError packet with nonce {}", packet_nonce);
                            }
                        },
                        ServerEvent::ConferenceLeaveError((packet_nonce, conference_id)) => {
                            if let Some(sent_event) = sent_packets.get(&packet_nonce) {
                                if let SentEvent::LeaveConference(expected_conference_id) = sent_event {
                                    if conference_id != *expected_conference_id {
                                        warn!("Received unexpected conference id {} from ConferenceLeaveError event, instead got {}", conference_id, expected_conference_id);
                                        continue;
                                    }
                                    warn!("Received a ConferenceLeaveError event for conference {}", conference_id);
                                    // ignore error and still remove conference
                                    ui_event_sender.send(UIEvent::ConferenceLeft(conference_id)).await.unwrap();
                                    sent_packets.remove(&packet_nonce);
                                    conferences.remove(&conference_id);
                                } else {
                                    warn!("Received unexpected packet with nonce {} from ConferenceLeaveError event, instead got {:?}", packet_nonce, sent_event);
                                }
                            } else {
                                warn!("Received unexpected ConferenceLeaveError packet with nonce {}", packet_nonce);
                            }
                        },
                        ServerEvent::MessageError((packet_nonce, conference_id)) => {
                            if let Some(sent_event) = sent_packets.get(&packet_nonce) {
                                if let SentEvent::SendMessage((expected_conference_id, message_id)) = sent_event {
                                    if conference_id != *expected_conference_id {
                                        warn!("Received unexpected conference id {} from MessageError event, instead got {}", conference_id, expected_conference_id);
                                        continue;
                                    }
                                    warn!("Received a MessageError event for conference {}", conference_id);
                                    ui_event_sender.send(UIEvent::MessageRejected((conference_id, *message_id))).await.unwrap();
                                    sent_packets.remove(&packet_nonce);
                                } else {
                                    warn!("Received unexpected packet with nonce {} from MessageError event, instead got {:?}", packet_nonce, sent_event);
                                }
                            } else {
                                warn!("Received unexpected MessageError packet with nonce {}", packet_nonce);
                            }
                        },
                    }
                },
                None => break,
            },
            message = message_receiver.next().fuse() => match message {
                // handle messages from 
                Some(message) => {
                    send_packets_last_index += 1;
                    let packet_nonce = send_packets_last_index;
                    let packet = ClientEvent::SendMessage((packet_nonce, message));
                    sent_packets.insert(packet_nonce, SentEvent::CreateConference);
                    client_event_sender.send(packet).await.unwrap();
                },
                None => break,
            },
            ui_event = ui_action_receiver.next().fuse() => match ui_event {
                // handle UI events
                Some(ui_event) => {
                    match ui_event {
                        UIAction::CreateConference(password) => {
                            let (password_hash, join_salt) = crypto::hash_password(password.as_bytes());
                            let encryption_salt = crypto::generate_salt();
                            send_packets_last_index += 1;
                            let packet_nonce = send_packets_last_index;
                            let packet = ClientEvent::CreateConference((packet_nonce, password_hash, join_salt, encryption_salt));

                            sent_packets.insert(packet_nonce, SentEvent::CreateConference);

                            client_event_sender.send(packet).await.unwrap();
                        },
                        UIAction::JoinConference((conference_id, password)) => {
                            send_packets_last_index += 1;
                            let packet_nonce = send_packets_last_index;
                            let packet = ClientEvent::GetConferenceJoinSalt((packet_nonce, conference_id));

                            sent_packets.insert(packet_nonce, SentEvent::GetConferenceJoinSalt((conference_id, password)));

                            client_event_sender.send(packet).await.unwrap();
                        },
                        UIAction::LeaveConference(conference_id) => {
                            send_packets_last_index += 1;
                            let packet_nonce = send_packets_last_index;
                            let packet = ClientEvent::LeaveConference((packet_nonce, conference_id));

                            sent_packets.insert(packet_nonce, SentEvent::LeaveConference(conference_id));

                            client_event_sender.send(packet).await.unwrap();
                        },
                        UIAction::SendMessage((conference_id, message_id, message)) => {
                            if let Some(mut conference_sender) = conferences.get(&conference_id) {
                                conference_sender.send(ConferenceEvent::OutboundMessage((message_id, message.as_bytes().to_vec()))).await.unwrap();
                            } else {
                                warn!("Attempted to send message to non-existent conference {}", conference_id);
                                ui_event_sender.send(UIEvent::MessageError((conference_id, message_id))).await.unwrap();
                            }
                        },
                        UIAction::Disconnect => {
                            send_packets_last_index += 1;
                            let packet_nonce = send_packets_last_index;
                            let packet = ClientEvent::Disconnect;

                            sent_packets.insert(packet_nonce, SentEvent::Disconnect); // todo might be useless

                            client_event_sender.send(packet).await.unwrap();
                            break;
                        },
                    }
                },
                None => break,
            },
        }
    }

    drop(conferences);
    drop(client_event_sender);
}

async fn create_conference(
    conference_id: ConferenceId,
    number_of_peers: NumberOfPeers,
    password: &[u8],
    encryption_salt: &[u8; 32],
    message_sender: Sender<Message>,
    ui_event_sender: Sender<UIEvent>,
) -> Sender<ConferenceEvent> {
    info!("Creating conference manager for conference {}", conference_id);
    let (sender, receiver) = mpsc::unbounded();
    let initial_encryption_key = crypto::hash_password_with_salt(password, encryption_salt);
    let mut manager = conference_manager::ConferenceManager::new(
        conference_id,
        number_of_peers,
        initial_encryption_key,
        receiver,
        message_sender,
        ui_event_sender
    );
    task::spawn(async move {
        if let Ok(()) = manager.start_conference_manager().await {
            warn!("Conference manager for conference {} exited successfully", conference_id);
        } else {
            warn!("Conference manager for conference {} exited with an error", conference_id);
        }
    });
    sender
}

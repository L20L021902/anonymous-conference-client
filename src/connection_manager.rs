use log::{debug, warn};
use async_native_tls::{TlsConnector, Certificate};
use async_std::{
    io::{BufReader, BufWriter},
    net::{TcpStream, ToSocketAddrs},
    prelude::*,
};
use futures::{select, FutureExt, SinkExt, AsyncReadExt, AsyncWriteExt};
use crate::constants::{Result, Sender, Receiver, ServerEvent, ClientEvent, SERVER_NAME, PROTOCOL_HEADER, ServerToClientMessageTypePrimitive, ConferenceJoinSalt, ConferenceEncryptionSalt};

async fn start_connection_manager(
    server_address: impl ToSocketAddrs,
    mut server_event_sender: Sender<ServerEvent>,
    mut client_event_receiver: Receiver<ClientEvent>
) -> Result<()> {
    let stream = TcpStream::connect(server_address).await?;
    debug!("Connected to server");
    let stream = TlsConnector::new()
        .add_root_certificate(get_cert())
        .connect(SERVER_NAME, stream)
        .await?;
    debug!("TLS handshake complete");
    let (reader, writer) = stream.split();
    let mut buf_reader = BufReader::new(reader);
    let mut buf_writer = BufWriter::new(writer);
    let mut server_event_type: [u8; 1] = [0; 1];

    // Handshake
    handle_handshake(&mut buf_reader, &mut buf_writer).await?;

    loop {
        select! {
            s = async_std::io::ReadExt::read_exact(&mut buf_reader, &mut server_event_type).fuse() => match s {
                Ok(()) => {
                    let event = read_server_event(server_event_type[0], &mut buf_reader).await?;
                    server_event_sender.send(event).await?;
                },
                Err(e) => { return Err(e.into()); },
            },
            client_event = client_event_receiver.next().fuse() => match client_event {
                Some(event) => {
                    if !write_client_event(event, &mut buf_writer).await? {
                        break;
                    }
                },
                None => break,
            },
        }
    }

    Ok(())
}

async fn handle_handshake(reader: &mut (impl AsyncReadExt + Unpin), writer: &mut (impl AsyncWriteExt + Unpin)) -> Result<()> {
    let mut server_response: [u8; 1] = [0; 1];
    writer.write_all(PROTOCOL_HEADER).await?;
    writer.flush().await?;

    reader.read_exact(&mut server_response).await?;
    if server_response[0] != ServerToClientMessageTypePrimitive::HandshakeAcknowledged as u8 {
        return Err("Handshake failed".into());
    }

    debug!("Handshake complete");

    Ok(())
}

async fn read_server_event(event_type: u8, reader: &mut (impl AsyncReadExt + Unpin)) -> Result<ServerEvent> {
    if let Ok(server_event_type) = ServerToClientMessageTypePrimitive::try_from(event_type) {
        match server_event_type {
            ServerToClientMessageTypePrimitive::HandshakeAcknowledged => {
                warn!("Server sent unexpected handshake acknowledgement");
                Ok(ServerEvent::HandshakeAcknowledged)
            },
            ServerToClientMessageTypePrimitive::ConferenceCreated => {
                let mut buffer: [u8; 4] = [0; 4];
                reader.read_exact(&mut buffer).await?;
                let nonce = u32::from_be_bytes(buffer);
                reader.read_exact(&mut buffer).await?;
                let conference_id = u32::from_be_bytes(buffer);
                Ok(ServerEvent::ConferenceCreated((nonce, conference_id)))
            },
            ServerToClientMessageTypePrimitive::ConferenceJoinSalt => {
                let mut buffer_small: [u8; 4] = [0; 4];
                let mut join_salt: ConferenceJoinSalt = [0; 32];
                reader.read_exact(&mut buffer_small).await?;
                let nonce = u32::from_be_bytes(buffer_small);
                reader.read_exact(&mut buffer_small).await?;
                let conference_id = u32::from_be_bytes(buffer_small);
                reader.read_exact(&mut join_salt).await?;
                Ok(ServerEvent::ConferenceJoinSalt((nonce, conference_id, join_salt)))
            },
            ServerToClientMessageTypePrimitive::ConferenceJoined => {
                let mut buffer: [u8; 4] = [0; 4];
                reader.read_exact(&mut buffer).await?;
                let nonce = u32::from_be_bytes(buffer);
                reader.read_exact(&mut buffer).await?;
                let conference_id = u32::from_be_bytes(buffer);
                reader.read_exact(&mut buffer).await?;
                let number_of_peers = u32::from_be_bytes(buffer);
                let mut buffer_large: ConferenceEncryptionSalt = [0; 32];
                reader.read_exact(&mut buffer_large).await?;
                Ok(ServerEvent::ConferenceJoined((nonce, conference_id, number_of_peers, buffer_large)))
            },
            ServerToClientMessageTypePrimitive::ConferenceLeft => {
                let mut buffer: [u8; 4] = [0; 4];
                reader.read_exact(&mut buffer).await?;
                let nonce = u32::from_be_bytes(buffer);
                reader.read_exact(&mut buffer).await?;
                let conference_id = u32::from_be_bytes(buffer);
                Ok(ServerEvent::ConferenceLeft((nonce, conference_id)))
            },
            ServerToClientMessageTypePrimitive::MessageAccepted => {
                let mut buffer: [u8; 4] = [0; 4];
                reader.read_exact(&mut buffer).await?;
                let nonce = u32::from_be_bytes(buffer);
                reader.read_exact(&mut buffer).await?;
                let conference_id = u32::from_be_bytes(buffer);
                Ok(ServerEvent::MessageAccepted((nonce, conference_id)))
            },
            ServerToClientMessageTypePrimitive::IncomingMessage => {
                let mut buffer: [u8; 4] = [0; 4];
                reader.read_exact(&mut buffer).await?;
                let conference_id = u32::from_be_bytes(buffer);
                reader.read_exact(&mut buffer).await?;
                let message_length = u32::from_be_bytes(buffer);
                let mut message = Vec::with_capacity(message_length as usize);
                reader.read_exact(&mut message).await?;
                Ok(ServerEvent::IncomingMessage((conference_id, message)))
            },
            ServerToClientMessageTypePrimitive::ConferenceRestructuring => {
                let mut buffer: [u8; 4] = [0; 4];
                reader.read_exact(&mut buffer).await?;
                let conference_id = u32::from_be_bytes(buffer);
                reader.read_exact(&mut buffer).await?;
                let number_of_peers = u32::from_be_bytes(buffer);
                Ok(ServerEvent::ConferenceRestructuring((conference_id, number_of_peers)))
            },
            ServerToClientMessageTypePrimitive::GeneralError => {
                Ok(ServerEvent::GeneralError)
            },
            ServerToClientMessageTypePrimitive::ConferenceCreationError => {
                let mut buffer: [u8; 4] = [0; 4];
                reader.read_exact(&mut buffer).await?;
                let nonce = u32::from_be_bytes(buffer);
                Ok(ServerEvent::ConferenceCreationError(nonce))
            },
            ServerToClientMessageTypePrimitive::ConferenceJoinSaltError => {
                let mut buffer_small: [u8; 4] = [0; 4];
                reader.read_exact(&mut buffer_small).await?;
                let nonce = u32::from_be_bytes(buffer_small);
                reader.read_exact(&mut buffer_small).await?;
                let conference_id = u32::from_be_bytes(buffer_small);
                Ok(ServerEvent::ConferenceJoinSaltError((nonce, conference_id)))
            },
            ServerToClientMessageTypePrimitive::ConferenceJoinError => {
                let mut buffer: [u8; 4] = [0; 4];
                reader.read_exact(&mut buffer).await?;
                let nonce = u32::from_be_bytes(buffer);
                reader.read_exact(&mut buffer).await?;
                let conference_id = u32::from_be_bytes(buffer);
                Ok(ServerEvent::ConferenceJoinError((nonce, conference_id)))
            },
            ServerToClientMessageTypePrimitive::ConferenceLeaveError => {
                let mut buffer: [u8; 4] = [0; 4];
                reader.read_exact(&mut buffer).await?;
                let nonce = u32::from_be_bytes(buffer);
                reader.read_exact(&mut buffer).await?;
                let conference_id = u32::from_be_bytes(buffer);
                Ok(ServerEvent::ConferenceLeaveError((nonce, conference_id)))
            },
            ServerToClientMessageTypePrimitive::MessageError => {
                let mut buffer: [u8; 4] = [0; 4];
                reader.read_exact(&mut buffer).await?;
                let nonce = u32::from_be_bytes(buffer);
                reader.read_exact(&mut buffer).await?;
                let conference_id = u32::from_be_bytes(buffer);
                Ok(ServerEvent::MessageError((nonce, conference_id)))
            },
        }
    } else {
        Err("Invalid server event type".into())
    }
}

/// Write a client event to the server, returning whether the connection should be kept open
async fn write_client_event(event: ClientEvent, writer: &mut (impl AsyncWriteExt + Unpin)) -> Result<bool> {
    writer.write_all(&[event.value()]).await?;
    match event {
        ClientEvent::CreateConference((nonce, password_hash, join_salt, encryption_salt)) => {
            writer.write_all(&nonce.to_be_bytes()).await?;
            writer.write_all(&password_hash).await?;
            writer.write_all(&join_salt).await?;
            writer.write_all(&encryption_salt).await?;
        },
        ClientEvent::GetConferenceJoinSalt((nonce, conference_id)) => {
            writer.write_all(&nonce.to_be_bytes()).await?;
            writer.write_all(&conference_id.to_be_bytes()).await?;
        },
        ClientEvent::JoinConference((nonce, conference_id, password_hash)) => {
            writer.write_all(&nonce.to_be_bytes()).await?;
            writer.write_all(&conference_id.to_be_bytes()).await?;
            writer.write_all(&password_hash).await?;
        },
        ClientEvent::LeaveConference((nonce, conference_id)) => {
            writer.write_all(&nonce.to_be_bytes()).await?;
            writer.write_all(&conference_id.to_be_bytes()).await?;
        },
        ClientEvent::SendMessage((nonce, message)) => {
            writer.write_all(&nonce.to_be_bytes()).await?;
            writer.write_all(&message.conference.to_be_bytes()).await?;
            writer.write_all(&message.message.len().to_be_bytes()).await?;
            writer.write_all(&message.message).await?;
        },
        ClientEvent::Disconnect => {
            writer.flush().await?;
            return Ok(false);
        },
    }

    writer.flush().await?;
    Ok(true)
}

fn get_cert() -> Certificate {
    debug!("Loading certificate");
    Certificate::from_pem(include_bytes!("../certs/certificate.pem")).expect("Invalid certificate")
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_std::task;
    use futures::channel::mpsc;
    use crate::constants::Result;

    #[async_std::test]
    async fn test_start_connection_manager() -> Result<()> {
        let (server_event_sender, _server_event_receiver) = mpsc::unbounded();
        let (mut client_event_sender, client_event_receiver) = mpsc::unbounded();
        let server_address = "localhost:7667";
        client_event_sender.send(ClientEvent::Disconnect).await?;
        task::block_on(start_connection_manager(server_address, server_event_sender, client_event_receiver))?;
        Ok(())
    }
}


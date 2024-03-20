use log::debug;
use async_native_tls::{TlsConnector, Certificate};
use async_std::{
    io::{BufReader, BufWriter},
    net::{TcpStream, ToSocketAddrs},
    prelude::*,
};
use futures::{select, FutureExt, AsyncBufRead, SinkExt, AsyncReadExt, AsyncWriteExt};
use crate::constants::{Result, Sender, Receiver, ServerEvent, ClientEvent, SERVER_NAME, PROTOCOL_HEADER, ServerToClientMessageType};

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
    if server_response[0] != ServerToClientMessageType::HandshakeAcknowledged as u8 {
        return Err("Handshake failed".into());
    }

    debug!("Handshake complete");

    Ok(())
}

async fn read_server_event(event_type: u8, reader: &mut (impl AsyncBufRead + Unpin)) -> Result<ServerEvent> {
    todo!();
}

/// Write a client event to the server, returning whether the connection should be kept open
async fn write_client_event(event: ClientEvent, writer: &mut (impl AsyncWriteExt + Unpin)) -> Result<bool> {
    match event {
        ClientEvent::Disconnect => {
            writer.write_all(&[ClientEvent::Disconnect as u8]).await?;
            writer.flush().await?;
            return Ok(false);
        },
        _ => {
            todo!();
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


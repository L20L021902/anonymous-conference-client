use async_std::{
    io::{BufReader, BufWriter},
    net::{TcpStream, ToSocketAddrs},
    prelude::*,
};
use futures::{select, FutureExt, AsyncBufRead, SinkExt, AsyncWrite};
use crate::constants::{Result, Sender, Receiver, ServerEvent, ClientEvent};

async fn start_connection_manager(
    server_address: impl ToSocketAddrs,
    mut server_event_sender: Sender<ServerEvent>,
    mut client_event_receiver: Receiver<ClientEvent>
) -> Result<()> {
    let stream = TcpStream::connect(server_address).await?;
    let mut buf_reader = BufReader::new(stream.clone());
    let mut buf_writer = BufWriter::new(stream);
    let mut server_event_type: [u8; 1] = [0; 1];

    loop {
        select! {
            s = buf_reader.read_exact(&mut server_event_type).fuse() => match s {
                Ok(()) => {
                    let event = read_server_event(server_event_type[0], &mut buf_reader).await?;
                    server_event_sender.send(event).await?;
                },
                Err(e) => { return Err(e.into()); },
            },
            client_event = client_event_receiver.next().fuse() => match client_event {
                Some(event) => {
                    write_client_event(event, &mut buf_writer).await?;
                },
                None => break,
            },
        }
    }

    Ok(())
}

async fn read_server_event(event_type: u8, reader: &mut (impl AsyncBufRead + Unpin)) -> Result<ServerEvent> {
    todo!();
}

async fn write_client_event(event: ClientEvent, writer: &mut (impl AsyncWrite + Unpin)) -> Result<()> {
    // remember to writer.flush().await?;
    todo!();
}

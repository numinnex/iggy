use crate::handlers::STATUS_OK;
use anyhow::Result;
use std::net::SocketAddr;
use streaming::stream_error::StreamError;
use streaming::system::System;
use tokio::net::UdpSocket;

pub const COMMAND: &[u8] = &[12];
const LENGTH: usize = 4;

pub async fn handle(
    input: &[u8],
    socket: &UdpSocket,
    address: SocketAddr,
    system: &mut System,
) -> Result<(), StreamError> {
    if input.len() != LENGTH {
        return Err(StreamError::InvalidCommand);
    }

    let id = u32::from_le_bytes(input[..4].try_into().unwrap());
    system.stream.delete_topic(id).await?;
    socket.send_to(STATUS_OK, address).await?;
    Ok(())
}
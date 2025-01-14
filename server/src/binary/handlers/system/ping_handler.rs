use crate::binary::sender::Sender;
use crate::streaming::session::Session;
use anyhow::Result;
use iggy::error::Error;
use iggy::system::ping::Ping;
use tracing::debug;

pub async fn handle(
    command: &Ping,
    sender: &mut dyn Sender,
    session: &Session,
) -> Result<(), Error> {
    debug!("session: {session}, command: {command}");
    sender.send_empty_ok_response().await?;
    Ok(())
}

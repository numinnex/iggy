use crate::binary;
use crate::client::MessageClient;
use crate::error::Error;
use crate::message::Message;
use crate::quic::client::QuicClient;
use async_trait::async_trait;
use shared::messages::poll_messages::PollMessages;
use shared::messages::send_messages::SendMessages;
use shared::offsets::store_offset::StoreOffset;

#[async_trait]
impl MessageClient for QuicClient {
    async fn poll_messages(&self, command: PollMessages) -> Result<Vec<Message>, Error> {
        binary::messages::poll_messages(self, command).await
    }

    async fn send_messages(&self, command: SendMessages) -> Result<(), Error> {
        binary::messages::send_messages(self, command).await
    }

    async fn store_offset(&self, command: StoreOffset) -> Result<(), Error> {
        binary::messages::store_offset(self, command).await
    }
}
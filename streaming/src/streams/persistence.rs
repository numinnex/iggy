use crate::error::Error;
use crate::streams::stream::Stream;
use crate::topics::topic::Topic;
use std::path::Path;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::info;

impl Stream {
    pub async fn persist(&self) -> Result<(), Error> {
        if Path::new(&self.path).exists() {
            return Err(Error::StreamAlreadyExists(self.id));
        }

        if !Path::new(&self.path).exists() && std::fs::create_dir(&self.path).is_err() {
            return Err(Error::CannotCreateStreamDirectory(self.id));
        }

        if !Path::new(&self.topics_path).exists() && std::fs::create_dir(&self.topics_path).is_err()
        {
            return Err(Error::CannotCreateTopicsDirectory);
        }

        let stream_info_file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(&self.info_path)
            .await;

        if stream_info_file.is_err() {
            return Err(Error::CannotCreateStreamInfo(self.id));
        }

        if stream_info_file
            .unwrap()
            .write_all(self.name.as_bytes())
            .await
            .is_err()
        {
            return Err(Error::CannotUpdateStreamInfo(self.id));
        }

        Ok(())
    }

    pub async fn persist_messages(&mut self) -> Result<(), Error> {
        for topic in self.get_topics_mut() {
            topic.persist_messages().await?;
        }

        Ok(())
    }

    pub async fn load(&mut self) -> Result<(), Error> {
        info!("Loading stream with ID: {} from disk...", &self.id);
        if !Path::new(&self.path).exists() {
            return Err(Error::StreamNotFound(self.id));
        }

        let stream_info_file = OpenOptions::new().read(true).open(&self.info_path).await;
        if stream_info_file.is_err() {
            return Err(Error::CannotOpenStreamInfo(self.id));
        }

        let mut stream_info = String::new();
        if stream_info_file
            .unwrap()
            .read_to_string(&mut stream_info)
            .await
            .is_err()
        {
            return Err(Error::CannotReadStreamInfo(self.id));
        }

        self.name = stream_info;
        let topics = std::fs::read_dir(&self.topics_path).unwrap();
        for topic in topics {
            let topic_id = topic
                .unwrap()
                .file_name()
                .into_string()
                .unwrap()
                .parse::<u32>()
                .unwrap();
            let mut topic = Topic::empty(topic_id, &self.topics_path, self.config.topic.clone());
            topic.load().await?;
            self.topics.insert(topic_id, topic);
        }

        info!(
            "Loaded stream: '{}' with ID: {} from disk.",
            &self.name, &self.id
        );

        Ok(())
    }
}
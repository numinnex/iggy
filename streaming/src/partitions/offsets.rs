use crate::partitions::partition::{ConsumerOffset, Partition};
use crate::polling_consumer::PollingConsumer;
use crate::utils::file;
use sdk::consumer_type::ConsumerType;
use sdk::error::Error;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::sync::RwLock;
use tracing::{error, trace};

impl Partition {
    pub async fn get_offset(&self, consumer: PollingConsumer) -> Result<u64, Error> {
        trace!(
            "Getting offset for {}, partition: {}, current: {}...",
            consumer,
            self.id,
            self.current_offset
        );

        let (consumer_offsets, consumer_id) = match consumer {
            PollingConsumer::Consumer(consumer_id) => {
                (self.consumer_offsets.read().await, consumer_id)
            }
            PollingConsumer::ConsumerGroup(consumer_group_id, _) => {
                (self.consumer_group_offsets.read().await, consumer_group_id)
            }
        };

        let consumer_offset = consumer_offsets.offsets.get(&consumer_id);
        if let Some(consumer_offset) = consumer_offset {
            let consumer_offset = consumer_offset.read().await;
            return Ok(consumer_offset.offset);
        }

        Ok(0)
    }

    pub async fn store_offset(&self, consumer: PollingConsumer, offset: u64) -> Result<(), Error> {
        trace!(
            "Storing offset: {} for {}, partition: {}, current: {}...",
            offset,
            consumer,
            self.id,
            self.current_offset
        );
        if offset > self.current_offset {
            return Err(Error::InvalidOffset(offset));
        }

        // This scope is required to avoid the potential deadlock by acquiring read lock and then write lock.
        {
            let (consumer_offsets, consumer_id) = match consumer {
                PollingConsumer::Consumer(consumer_id) => {
                    (self.consumer_offsets.read().await, consumer_id)
                }
                PollingConsumer::ConsumerGroup(consumer_group_id, _) => {
                    (self.consumer_group_offsets.read().await, consumer_group_id)
                }
            };
            let consumer_offset = consumer_offsets.offsets.get(&consumer_id);
            if let Some(consumer_offset) = consumer_offset {
                let mut consumer_offset = consumer_offset.write().await;
                consumer_offset.offset = offset;
                self.storage.partition.save_offset(&consumer_offset).await?;
                return Ok(());
            }
        }

        let (mut consumer_offsets, consumer_id, path) = match consumer {
            PollingConsumer::Consumer(consumer_id) => (
                self.consumer_offsets.write().await,
                consumer_id,
                &self.consumer_offsets_path,
            ),
            PollingConsumer::ConsumerGroup(consumer_group_id, _) => (
                self.consumer_group_offsets.write().await,
                consumer_group_id,
                &self.consumer_group_offsets_path,
            ),
        };

        let path = format!("{}/{}", path, consumer_id);
        let consumer_offset = ConsumerOffset {
            consumer_id,
            offset,
            path,
        };
        self.storage.partition.save_offset(&consumer_offset).await?;
        consumer_offsets
            .offsets
            .insert(consumer_id, RwLock::new(consumer_offset));
        Ok(())
    }

    pub async fn load_offsets(&mut self, consumer_type: ConsumerType) -> Result<(), Error> {
        trace!(
                "Loading consumer offsets for partition with ID: {} for topic with ID: {} and stream with ID: {}...",
                self.id,
                self.topic_id,
                self.stream_id
            );

        let (path, mut offsets) = match consumer_type {
            ConsumerType::Consumer => (
                &self.consumer_offsets_path,
                self.consumer_offsets.write().await,
            ),
            ConsumerType::ConsumerGroup => (
                &self.consumer_group_offsets_path,
                self.consumer_group_offsets.write().await,
            ),
        };

        let dir_entries = fs::read_dir(&path).await;
        if dir_entries.is_err() {
            return Err(Error::CannotReadConsumerOffsets(self.id));
        }

        let mut dir_entries = dir_entries.unwrap();
        while let Some(dir_entry) = dir_entries.next_entry().await.unwrap_or(None) {
            let metadata = dir_entry.metadata().await;
            if metadata.is_err() || metadata.unwrap().is_dir() {
                continue;
            }

            let name = dir_entry.file_name().into_string().unwrap();
            let consumer_id = name.parse::<u32>();
            if consumer_id.is_err() {
                error!("Invalid consumer ID file with name: '{}'.", name);
                continue;
            }

            let path = dir_entry.path();
            let path = path.to_str();
            if path.is_none() {
                error!("Invalid consumer ID path for file with name: '{}'.", name);
                continue;
            }

            let path = path.unwrap().to_string();
            let consumer_id = consumer_id.unwrap();
            let mut file = file::open(&path).await?;
            let offset = file.read_u64_le().await?;
            offsets.offsets.insert(
                consumer_id,
                RwLock::new(ConsumerOffset {
                    consumer_id,
                    offset,
                    path,
                }),
            );

            trace!(
                "Loaded consumer offset: {} for {} ID: {} and partition with ID: {} for topic with ID: {} and stream with ID: {}.",
                offset,
                consumer_type,
                consumer_id,
                self.id,
                self.topic_id,
                self.stream_id
            );
        }

        Ok(())
    }
}

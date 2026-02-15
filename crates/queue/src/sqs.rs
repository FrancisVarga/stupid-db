//! AWS SQS consumer implementation.

use async_trait::async_trait;
use aws_credential_types::Credentials;
use aws_sdk_sqs::config::BehaviorVersion;
use aws_sdk_sqs::types::QueueAttributeName;
use aws_sdk_sqs::Client;
use chrono::{TimeZone, Utc};
use tracing::{debug, info};

use stupid_core::config::{AwsConfig, QueueConfig};

use crate::consumer::{QueueConsumer, QueueHealth, QueueMessage};
use crate::error::QueueError;

/// SQS-backed queue consumer.
pub struct SqsConsumer {
    client: Client,
    queue_url: String,
    dlq_url: Option<String>,
    visibility_timeout_secs: i32,
}

impl SqsConsumer {
    /// Create a new SQS consumer from project config.
    pub async fn new(aws: &AwsConfig, queue: &QueueConfig) -> Result<Self, QueueError> {
        let region = aws_sdk_sqs::config::Region::new(aws.region.clone());

        // Build SQS client config directly — do NOT use aws_config::defaults()
        // because it reads AWS_ENDPOINT_URL from the environment, which may point
        // to S3 and would route all SQS requests to the wrong service.
        let mut sqs_config = aws_sdk_sqs::Config::builder()
            .region(region.clone())
            .behavior_version(BehaviorVersion::latest());

        // Use static credentials if provided (local dev / explicit config).
        if let (Some(key_id), Some(secret)) =
            (&aws.access_key_id, &aws.secret_access_key)
        {
            let creds = Credentials::new(
                key_id,
                secret,
                aws.session_token.clone(),
                None,
                "stupid-queue-static",
            );
            sqs_config = sqs_config.credentials_provider(creds);
        }

        // Only apply endpoint override if QUEUE_AWS_ENDPOINT_URL is explicitly set.
        if let Some(ref endpoint) = aws.endpoint_url {
            if !endpoint.is_empty() {
                let url = if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
                    endpoint.clone()
                } else {
                    format!("https://{endpoint}")
                };
                sqs_config = sqs_config.endpoint_url(&url);
            }
        }

        let client = Client::from_conf(sqs_config.build());

        info!(
            queue_url = %queue.queue_url,
            region = %aws.region,
            "SQS consumer initialized"
        );

        Ok(Self {
            client,
            queue_url: queue.queue_url.clone(),
            dlq_url: queue.dlq_url.clone(),
            visibility_timeout_secs: queue.visibility_timeout_secs as i32,
        })
    }
}

#[async_trait]
impl QueueConsumer for SqsConsumer {
    async fn poll_batch(&self, max_messages: u32) -> Result<Vec<QueueMessage>, QueueError> {
        // SQS caps at 10 messages per request.
        let capped = max_messages.min(10) as i32;

        debug!(max_messages = capped, "Polling SQS");

        let resp = self
            .client
            .receive_message()
            .queue_url(&self.queue_url)
            .max_number_of_messages(capped)
            .wait_time_seconds(20)
            .visibility_timeout(self.visibility_timeout_secs)
            .message_system_attribute_names(aws_sdk_sqs::types::MessageSystemAttributeName::All)
            .send()
            .await
            .map_err(|e| QueueError::Connection(format!("SQS receive failed: {e:?}")))?;

        let sqs_messages = resp.messages.unwrap_or_default();
        debug!(count = sqs_messages.len(), "Received SQS messages");

        let mut messages = Vec::with_capacity(sqs_messages.len());
        for msg in sqs_messages {
            let id = msg
                .message_id()
                .unwrap_or("unknown")
                .to_string();

            let body = msg
                .body()
                .unwrap_or("")
                .to_string();

            let receipt_handle = msg
                .receipt_handle()
                .ok_or_else(|| QueueError::Parse("missing receipt handle".into()))?
                .to_string();

            // Extract timestamp from SentTimestamp attribute (epoch millis).
            let timestamp = msg
                .attributes()
                .and_then(|attrs| {
                    attrs
                        .get(&aws_sdk_sqs::types::MessageSystemAttributeName::SentTimestamp)
                })
                .and_then(|ts| ts.parse::<i64>().ok())
                .and_then(|ms| Utc.timestamp_millis_opt(ms).single())
                .unwrap_or_else(Utc::now);

            // Extract receive count from ApproximateReceiveCount attribute.
            let attempt_count = msg
                .attributes()
                .and_then(|attrs| {
                    attrs.get(
                        &aws_sdk_sqs::types::MessageSystemAttributeName::ApproximateReceiveCount,
                    )
                })
                .and_then(|c| c.parse::<u32>().ok())
                .unwrap_or(1);

            messages.push(QueueMessage {
                id,
                body,
                receipt_handle,
                timestamp,
                attempt_count,
            });
        }

        Ok(messages)
    }

    async fn ack(&self, receipt_handle: &str) -> Result<(), QueueError> {
        debug!(receipt_handle, "Acking SQS message");

        self.client
            .delete_message()
            .queue_url(&self.queue_url)
            .receipt_handle(receipt_handle)
            .send()
            .await
            .map_err(|e| QueueError::Ack(format!("SQS delete failed: {e:?}")))?;

        Ok(())
    }

    async fn nack(&self, receipt_handle: &str) -> Result<(), QueueError> {
        debug!(receipt_handle, "Nacking SQS message (visibility=0)");

        self.client
            .change_message_visibility()
            .queue_url(&self.queue_url)
            .receipt_handle(receipt_handle)
            .visibility_timeout(0)
            .send()
            .await
            .map_err(|e| QueueError::Provider(format!("SQS visibility change failed: {e:?}")))?;

        Ok(())
    }

    async fn health_check(&self) -> Result<QueueHealth, QueueError> {
        let resp = self
            .client
            .get_queue_attributes()
            .queue_url(&self.queue_url)
            .attribute_names(QueueAttributeName::ApproximateNumberOfMessages)
            .send()
            .await
            .map_err(|e| QueueError::Connection(format!("SQS health check failed: {e:?}")))?;

        let count = resp
            .attributes()
            .and_then(|attrs| attrs.get(&QueueAttributeName::ApproximateNumberOfMessages))
            .and_then(|v| v.parse::<u64>().ok());

        Ok(QueueHealth {
            connected: true,
            approximate_message_count: count,
            provider: "sqs".to_string(),
        })
    }

    async fn dlq_depth(&self) -> Result<Option<u64>, QueueError> {
        let dlq_url = match &self.dlq_url {
            Some(url) => url,
            None => return Ok(None),
        };

        let resp = self
            .client
            .get_queue_attributes()
            .queue_url(dlq_url)
            .attribute_names(QueueAttributeName::ApproximateNumberOfMessages)
            .send()
            .await
            .map_err(|e| QueueError::Connection(format!("SQS DLQ check failed: {e:?}")))?;

        let count = resp
            .attributes()
            .and_then(|attrs| attrs.get(&QueueAttributeName::ApproximateNumberOfMessages))
            .and_then(|v| v.parse::<u64>().ok());

        Ok(count)
    }
}

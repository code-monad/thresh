use crate::error::Error;
use config::{Config as ConfigFile, File};
use rumqttc::QoS;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub websocket: WebSocketConfig,
    pub mqtt: MqttConfig,
    pub processing: ProcessingConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WebSocketConfig {
    pub endpoint: String,
    pub retry_interval: u64,
    pub max_retries: u32,
    pub ping_interval: u64,
    #[serde(default = "default_topics")]
    pub topics: Vec<SubscriptionTopic>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SubscriptionTopic {
    pub name: String,
    pub enabled: bool,
    #[serde(default)]
    pub id: Option<u32>,
    #[serde(default = "default_mqtt_topic")]
    pub mqtt_topic: String,
}

fn default_mqtt_topic() -> String {
    "ckb.default".to_string()
}

fn default_topics() -> Vec<SubscriptionTopic> {
    vec![
        SubscriptionTopic {
            name: "proposed_transaction".to_string(),
            enabled: true,
            id: Some(2),
            mqtt_topic: "ckb.transactions.proposed".to_string(),
        },
        SubscriptionTopic {
            name: "new_tip_block".to_string(),
            enabled: false,
            id: Some(3),
            mqtt_topic: "ckb.blocks.new_tip".to_string(),
        },
    ]
}
pub type Result<T, E = Error> = std::result::Result<T, E>;
fn deserialize_qos<'de, D>(deserializer: D) -> Result<QoS, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let qos_value = i32::deserialize(deserializer)?;
    match qos_value {
        0 => Ok(QoS::AtMostOnce),
        1 => Ok(QoS::AtLeastOnce),
        2 => Ok(QoS::ExactlyOnce),
        _ => Ok(QoS::AtLeastOnce), // Default to QoS 1 for invalid values
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct MqttConfig {
    pub broker: String,    // MQTT broker address
    pub port: u16,         // MQTT broker port
    pub client_id: String, // Client identifier
    #[serde(deserialize_with = "deserialize_qos")]
    pub qos: QoS, // Quality of Service level
    pub reconnect_delay: u64, // Reconnection delay in seconds
    pub batch_size: usize, // Message batch size
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProcessingConfig {
    pub workers: usize,    // Number of worker threads
    pub queue_size: usize, // Message queue size
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let settings = ConfigFile::builder()
            .add_source(File::with_name(path.as_ref().to_str().unwrap()))
            .build()
            .map_err(Error::Config)?;

        settings.try_deserialize().map_err(Error::Config)
    }
}

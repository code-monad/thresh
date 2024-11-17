#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate::*;
    use test_log::test;

    #[test(tokio::test)]
    async fn test_mqtt_publisher_batch_processing() {
        let (tx, rx) = mpsc::channel(100);
        let config = MqttConfig {
            broker: "localhost".to_string(),
            port: 1883,
            topic: "test".to_string(),
            client_id: "test".to_string(),
            qos: 1,
            reconnect_delay: 5,
            batch_size: 2,
        };

        let mut publisher = MqttPublisher::new(
            config.broker,
            config.port,
            config.client_id,
            config.topic,
            rx,
        )
        .await
        .unwrap();

        // Send test messages
        tx.send("message1".to_string()).await.unwrap();
        tx.send("message2".to_string()).await.unwrap();

        // Run publisher for a short time
        tokio::time::timeout(Duration::from_secs(1), publisher.run())
            .await
            .unwrap_err();
    }

    #[test(tokio::test)]
    async fn test_mqtt_reconnection() {
        let (tx, rx) = mpsc::channel(100);
        let config = MqttConfig::default();

        let mut publisher = MqttPublisher::new(
            "non-existent-broker".to_string(),
            1883,
            "test".to_string(),
            "test".to_string(),
            rx,
        )
        .await
        .unwrap();

        // Test should handle connection failure gracefully
        let result = publisher.run().await;
        assert!(result.is_err());
    }
}

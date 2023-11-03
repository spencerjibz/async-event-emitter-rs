#[cfg(test)]

mod async_event_emitter {
    use anyhow::Ok;
    use async_event_emitter::AsyncEventEmitter;
    use futures::lock::Mutex;
    use lazy_static::lazy_static;
    use serde::{Deserialize, Serialize};

    lazy_static! {
        // Export the emitter with `pub` keyword
        pub static ref EVENT_EMITTER: Mutex<AsyncEventEmitter> = Mutex::new(AsyncEventEmitter::new());
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
    struct Date {
        month: String,
        day: String,
    }
    #[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
    struct Time {
        hour: String,
        minute: String,
    }
    #[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
    struct DateTime(Date, Time);

    #[tokio::test]

    async fn test_async_event() -> anyhow::Result<()> {
        let mut event_emitter = AsyncEventEmitter::new();

        let date = Date {
            month: "January".to_string(),
            day: "Tuesday".to_string(),
        };

        event_emitter.on("LOG_DATE", |_date: Date| {
            async move { /*Do something here */ }
        });

        event_emitter.on("LOG_DATE", |date: Date| async move {
            println!(" emitted data: {:#?}", date)
        });
        event_emitter.emit("LOG_DATE", date).await?;
        println!("{:#?}", event_emitter);
        assert!(event_emitter.listeners.get("LOG_DATE").is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_emit_multiple_args() -> anyhow::Result<()> {
        let mut event_emitter = AsyncEventEmitter::new();
        let name = "LOG_DATE".to_string();
        let payload = (
            Date {
                month: "January".to_string(),
                day: "Tuesday".to_string(),
            },
            name,
        );

        let copy = payload.clone();
        event_emitter.on("LOG_DATE", move |tup: (Date, String)| {
            assert_eq!(tup, copy);
            async move {}
        });

        event_emitter.emit("LOG_DATE", payload).await?;

        Ok(())
    }

    #[tokio::test]
    async fn listens_once_with_multiple_emits() -> anyhow::Result<()> {
        let mut event_emitter = AsyncEventEmitter::new();
        let name = "LOG_DATE".to_string();
        event_emitter.once("LOG_DATE", |tup: (Date, String)| async move {
            println!("{:#?}", tup)
        });

        event_emitter
            .emit(
                "LOG_DATE",
                (
                    Date {
                        month: "January".to_string(),
                        day: "Tuesday".to_string(),
                    },
                    name.clone(),
                ),
            )
            .await?;
        event_emitter
            .emit(
                "LOG_DATE",
                (
                    Date {
                        month: "January".to_string(),
                        day: "Tuesday".to_string(),
                    },
                    name,
                ),
            )
            .await?;

        assert_eq!(event_emitter.listeners.len(), 1);
        if let Some(event) = event_emitter.listeners.get("LOG_DATE") {
            println!("{:?}", event)
        }

        Ok(())
    }
    #[tokio::test]
    async fn remove_listeners() -> anyhow::Result<()> {
        let mut event_emitter = AsyncEventEmitter::new();

        let _listener_id =
            event_emitter.on(
                "PING",
                |msg: String| async move { assert_eq!(&msg, "pong") },
            );

        event_emitter.emit("PING", String::from("pong")).await?;

        event_emitter.remove_listener(&_listener_id);

        if let Some(event_listeners) = event_emitter.listeners.get("PING") {
            assert!(event_listeners.is_empty())
        }

        Ok(())
    }

    #[tokio::test]
    #[should_panic]
    async fn panics_on_different_values_for_same_event() {
        let mut event_emitter = AsyncEventEmitter::new();

        event_emitter.on("value", |_v: Vec<u8>| async move {});

        event_emitter
            .emit::<&'static str>("value", "string")
            .await
            .unwrap();
        event_emitter.emit("value", 12).await.unwrap();
    }
    #[tokio::test]

    async fn global_event_emitter() {
        // We need to maintain a lock through the mutex so we can avoid data races
        EVENT_EMITTER
            .lock()
            .await
            .on("Hello", |v: String| async move { assert_eq!(&v, "world") });
        let _ = EVENT_EMITTER.lock().await.emit("Hello", "world").await;
    }
}

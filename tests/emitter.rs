mod tester {
    /// use
    #[cfg(feature = "use-async-std")]
    pub use async_std::test;
    #[cfg(not(feature = "use-async-std"))]
    pub use tokio::test;
}

#[cfg(test)]
mod async_event_emitter {
    use anyhow::Ok;
    use async_event_emitter::AsyncEventEmitter;
    use futures::FutureExt;
    use lazy_static::lazy_static;
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;

    lazy_static! {
        // Export the emitter with `pub` keyword
        pub static ref EVENT_EMITTER: AsyncEventEmitter = AsyncEventEmitter::new();
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

    #[tester::test]
    async fn test_async_event() -> anyhow::Result<()> {
        let event_emitter = AsyncEventEmitter::new();

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
        assert!(event_emitter.listeners.contains_key("LOG_DATE"));

        Ok(())
    }

    #[tester::test]
    async fn test_emit_multiple_args() -> anyhow::Result<()> {
        let event_emitter = AsyncEventEmitter::new();
        let time = Time {
            hour: "22".to_owned(),
            minute: "30".to_owned(),
        };
        let name = "LOG_DATE".to_string();
        let payload = (
            Date {
                month: "January".to_string(),
                day: "Tuesday".to_string(),
            },
            name,
            time,
        );

        let copy = payload.clone();
        event_emitter.on("LOG_DATE", move |tup: (Date, String, Time)| {
            assert_eq!(tup, copy);
            async move {}
        });

        event_emitter.emit("LOG_DATE", payload).await?;

        Ok(())
    }

    #[tester::test]
    async fn listens_once_with_multiple_emits() -> anyhow::Result<()> {
        let event_emitter = AsyncEventEmitter::new();
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
    #[tester::test]
    async fn remove_listeners() -> anyhow::Result<()> {
        let event_emitter = AsyncEventEmitter::new();
        let dt = DateTime(
            Date {
                month: "11".to_owned(),
                day: String::from("03"),
            },
            Time {
                hour: "12".to_owned(),
                minute: "50".to_owned(),
            },
        );
        let copy = dt.clone();

        let _listener_id =
            event_emitter.on(
                "PING",
                |msg: String| async move { assert_eq!(&msg, "pong") },
            );

        let _listener_two = event_emitter.on("DateTime", move |d: Vec<u8>| {
            let copy = dt.clone();
            async move {
                let a = bincode::serialize(&copy).unwrap();
                assert!(!a.is_empty());
                assert!(!d.is_empty())
            }
        });

        event_emitter.emit("PING", String::from("pong")).await?;
        event_emitter.emit("DateTime", copy).await?;

        if let Some(id) = event_emitter.remove_listener(&_listener_id) {
            assert_eq!(id, _listener_id)
        }

        if let Some(event_listeners) = event_emitter.listeners.get("PING") {
            assert!(event_listeners.is_empty())
        }
        assert!(event_emitter.remove_listener("some").is_none());

        Ok(())
    }

    #[tester::test]
    #[should_panic]
    async fn panics_on_different_values_for_same_event() {
        let event_emitter = AsyncEventEmitter::new();

        event_emitter.on("value", |_v: Vec<u8>| async move {});

        event_emitter
            .emit::<&'static str>("value", "string")
            .await
            .unwrap();
        event_emitter.emit("value", 12).await.unwrap();
    }
    #[tester::test]

    async fn global_event_emitter() {
        EVENT_EMITTER.on("Hello", |v: String| async move { assert_eq!(&v, "world") });
        let _ = EVENT_EMITTER.emit("Hello", "world").await;
    }

    // tests/listener_tests.rs

    use crate::tester;
    use async_event_emitter::AsyncListener;

    #[test]
    fn test_async_listener() {
        // Test cases for AsyncListener struct

        // Basic test with default values
        let listener = AsyncListener {
            callback: Arc::new(|_| async {}.boxed()),
            limit: None,
            id: "1".to_string(),
        };

        assert_eq!(listener.limit, None);
        assert_eq!(listener.id.clone(), "1");

        // Test with custom values
        let callback = Arc::new(|_| async {}.boxed());
        let limit = Some(10);
        let id = "my-id".to_string();

        let listener = AsyncListener {
            callback,
            limit,
            id: id.clone(),
        };

        assert_eq!(listener.limit, limit);
        assert_eq!(listener.id, id);
        println!("{listener:?}")

        // Add more test cases to cover edge cases
    }
}

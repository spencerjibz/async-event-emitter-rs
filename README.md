### async-event-emitter

[![Crates.io](https://img.shields.io/crates/v/async-event-emitter)](https://crates.io/crates/async-event-emitter)
[![docs.rs](https://img.shields.io/docsrs/async-event-emitter)](https://docs.rs/async-event-emitter/0.1.1/async_event_emitter/)
[![CI](https://github.com/spencerjibz/async-event-emitter-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/spencerjibz/async-event-emitter-rs/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/spencerjibz/async-event-emitter-rs/graph/badge.svg?token=WDGKRW604P)](https://codecov.io/gh/spencerjibz/async-event-emitter-rs)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

an Async implementation of the [`event-emitter-rs`](https://crates.io/crates/event-emitter-rs) crate

Allows you to subscribe to events with callbacks and also fire those events.
Events are in the form of (strings, value) and callbacks are in the form of closures that take in a value parameter;

#### Differences between this crate and [`event-emitter-rs`](https://crates.io/crates/event-emitter-rs)
 - This is an async implementation that works for all common async runtimes (Tokio, async-std and smol)
 - The listener methods ***(on and once)*** take a callback that returns a future instead of a merely a closure.
 - The emit methods executes each callback on each event by spawning a tokio task instead of a std::thread
 - This emitter is thread safe and can  also be used lock-free (supports interior mutability).

 ***Note***: To use strict return and event types, use [typed-emitter](https://crates.io/crates/typed-emitter), that crate solves [this issue](https://github.com/spencerjibz/async-event-emitter-rs/issues/31) too.  

#### Getting Started

```rust
    use async_event_emitter::AsyncEventEmitter;
    #[tokio::main]
    async fn main() {
        let event_emitter = AsyncEventEmitter::new();
        // This will print <"Hello world!"> whenever the <"Say Hello"> event is emitted
        event_emitter.on("Say Hello", |_: ()| async move { println!("Hello world!") });
        event_emitter.emit("Say Hello", ()).await.unwrap();
        // >> "Hello world!"
    }
```

#### Basic Usage

We can emit and listen to values of any type so long as they implement the Debug trait and serde's Serialize and Deserialize traits.
A single EventEmitter instance can have listeners to values of multiple types.

```rust
    use async_event_emitter::AsyncEventEmitter as EventEmitter;
    use serde::{Deserialize, Serialize};

    #[tokio::main]
    async fn main() -> anyhow::Result<()> {
        let event_emitter = EventEmitter::new();
        event_emitter.on("Add three", |number: f64| async move {
            println!("{}", number + 3.0)
        });

        event_emitter.emit("Add three", 5.0_f64).await?;
        event_emitter.emit("Add three", 4.0_f64).await?;

        // >> "8.0"
        // >> "7.0"

        // Using a more advanced value type such as a struct by implementing the serde traits
        #[derive(Serialize, Deserialize, Debug)]
        struct Date {
            month: String,
            day: String,
        }

        event_emitter.on(
            "LOG_DATE",
            |_date: Date| async move { println!("{_date:?}") },
        );
        event_emitter
            .emit(
                "LOG_DATE",
                Date {
                    month: "January".to_string(),
                    day: "Tuesday".to_string(),
                },
            )
            .await?;
        event_emitter
            .emit(
                "LOG_DATE",
                Date {
                    month: "February".to_string(),
                    day: "Tuesday".to_string(),
                },
            )
            .await?;
        // >> "Month: January - Day: Tuesday"
        // >> "Month: January - Day: Tuesday"

        Ok(())
    }
```

Removing listeners is also easy

```rust
    use async_event_emitter::AsyncEventEmitter as EventEmitter;
    let event_emitter = EventEmitter::new();

    let listener_id = event_emitter.on("Hello", |_: ()| async { println!("Hello World") });
    match event_emitter.remove_listener(&listener_id) {
        Some(listener_id) => println!("Removed event listener! {listener_id}"),
        None => println!("No event listener of that id exists"),
    }
```

#### Creating a Global EventEmitter

You'll likely want to have a single EventEmitter instance that can be shared across files;

After all, one of the main points of using an EventEmitter is to avoid passing down a value through several nested functions/types and having a global subscription service.

```rust
        // global_event_emitter.rs
        use async_event_emitter::AsyncEventEmitter;
        use lazy_static::lazy_static;

        // Use lazy_static! because the size of EventEmitter is not known at compile time
        lazy_static! {
        // Export the emitter with `pub` keyword
        pub static ref EVENT_EMITTER: AsyncEventEmitter = AsyncEventEmitter::new();
        }

        #[tokio::main]
        async fn main() -> anyhow::Result<()> {
            EVENT_EMITTER.on("Hello", |_: ()| async { println!("hello there!") });
            EVENT_EMITTER.emit("Hello", ()).await?;

            Ok(())
        }

        async fn random_function() {
            // When the <"Hello"> event is emitted in main.rs then print <"Random stuff!">
            EVENT_EMITTER
                .on("Hello", |_: ()| async { println!("Random stuff!") });
        }

```
#### Usage with other runtimes
 Check out the examples from the [typed version of this crate](https://docs.rs/typed-emitter/0.1.2/typed_emitter/#getting-started), just replace the emntter type.

 #### Testing
 Run the tests on this crate with all-features enabled as follows:
       ``` cargo test --all-features```


#### License

MIT License (MIT), see [LICENSE](LICENSE)

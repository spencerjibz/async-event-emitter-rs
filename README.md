### async-event-emitter
[![CI](https://github.com/spencerjibz/async-event-emitter-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/spencerjibz/async-event-emitter-rs/actions/workflows/ci.yml)
[![cov_ci](https://github.com/spencerjibz/async-event-emitter-rs/actions/workflows/codecov.yml/badge.svg)](https://github.com/spencerjibz/async-event-emitter-rs/actions/workflows/codecov.yml)
[![codecov](https://codecov.io/gh/spencerjibz/async-event-emitter-rs/graph/badge.svg?token=WDGKRW604P)](https://codecov.io/gh/spencerjibz/async-event-emitter-rs)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

an Async implementation of the [`event-emitter-rs`](https://crates.io/crates/event-emitter-rs) crate

Allows you to subscribe to events with callbacks and also fire those events.
Events are in the form of (strings, value) and callbacks are in the form of closures that take in a value parameter;

#### Differences between this crate and [`event-emitter-rs`](https://crates.io/crates/event-emitter-rs)

-    Emitted values should implement an extra trait (Debug) in addition to Serde's Serialize and Deserialize.
-    This is an async implementation, currently limited to tokio, but async-std will be added soon under a feature flag.
-    The listener methods **_(on and once)_** take a callback that returns a future instead of a merely a closure.
-    The emit methods executes each callback on each event by spawning a tokio task instead of a std::thread

#### Getting Started

```rust
use async_event_emitter::AsyncEventEmitter;

#[tokio::main]
async fn main() {
let mut event_emitter = AsyncEventEmitter::new();
// This will print <"Hello world!"> whenever the <"Say Hello"> event is emitted
event_emitter.on("Say Hello", |_:()|  async move { println!("Hello world!")});
event_emitter.emit("Say Hello", ()).await;
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
async fn main () {
let mut event_emitter = EventEmitter::new();
event_emitter.on("Add three", |number: f32| async move  {println!("{}", number + 3.0)});
event_emitter.emit("Add three", 5.0 as f32).await;
event_emitter.emit("Add three", 4.0 as f32).await;

// >> "8.0"
// >> "7.0"

// Using a more advanced value type such as a struct by implementing the serde traits
#[derive(Serialize, Deserialize,Debug)]
struct Date {
month: String,
day: String,
}

event_emitter.on("LOG_DATE", |date: Date|  async move {
println!("Month: {} - Day: {}", date.month, date.day)
});
event_emitter.emit("LOG_DATE", Date {
month: "January".to_string(),
day: "Tuesday".to_string()
}).await;
// >> "Month: January - Day: Tuesday"
}
```

Removing listeners is also easy

```rust
use async_event_emitter::AsyncEventEmitter as EventEmitter;
let mut event_emitter = EventEmitter::new();

let listener_id = event_emitter.on("Hello", |_: ()|  async {println!("Hello World")});
match event_emitter.remove_listener(&listener_id) {
Some(listener_id) => print!("Removed event listener!"),
None => print!("No event listener of that id exists")
}
```

#### Creating a Global EventEmitter

You'll likely want to have a single EventEmitter instance that can be shared across files;

After all, one of the main points of using an EventEmitter is to avoid passing down a value through several nested functions/types and having a global subscription service.

```rust
// global_event_emitter.rs
use lazy_static::lazy_static;
use futures::lock::Mutex;
use async_event_emitter::AsyncEventEmitter;

// Use lazy_static! because the size of EventEmitter is not known at compile time
lazy_static! {
// Export the emitter with `pub` keyword
pub static ref EVENT_EMITTER: Mutex<AsyncEventEmitter> = Mutex::new(AsyncEventEmitter::new());
}.

#[tokio::main]
async fn main() {
// We need to maintain a lock through the mutex so we can avoid data races
EVENT_EMITTER.lock().await.on("Hello", |_:()|  async {println!("hello there!")});
EVENT_EMITTER.lock().await.emit("Hello", ()).await;
}

async fn random_function() {
// When the <"Hello"> event is emitted in main.rs then print <"Random stuff!">
EVENT_EMITTER.lock().unwrap().on("Hello", |_: ()| async { println!("Random stuff!")});
}

```

#### License
MIT License (MIT), see [LICENSE](LICENSE)

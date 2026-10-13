/*!

        an Async implementation of the  [`event-emitter-rs`](https://crates.io/crates/event-emitter-rs) crate

        Allows you to subscribe to events with callbacks and also fire those events.
        Events are in the form of (strings, value) and callbacks are in the form of closures that take in a value parameter;

        ## Differences between this crate and [`event-emitter-rs`](https://crates.io/crates/event-emitter-rs)
        - This is an async implementation that works for all common async runtimes (Tokio, async-std and smol)
        - The listener methods ***(on and once)*** take a callback that returns a future instead of a merely a closure.
        - The emit methods executes each callback on each event by running async intra-task instead of spawning a std::thread
        - This emitter is thread safe and can  also be used lock-free (supports interior mutability).

        ***Note***: To use strict return and event types, use [typed-emitter](https://crates.io/crates/typed-emitter), that crate solves [this issue](https://github.com/spencerjibz/async-event-emitter-rs/issues/31) too.

        ## Getting Started

        ```
        use async_event_emitter::AsyncEventEmitter;
        #[tokio::main]
        async fn main() {
        let event_emitter = AsyncEventEmitter::new();
        // This will print <"Hello world!"> whenever the <"Say Hello"> event is emitted
        event_emitter.on("Say Hello", |_:()|  async move { println!("Hello world!")});
        event_emitter.emit("Say Hello", ()).await;
        // >> "Hello world!"

        }
        ```
        ## Basic Usage
        We can emit and listen to values of any type so long as they implement serde's Serialize and Deserialize traits.
        A single EventEmitter instance can have listeners to values of multiple types.

        ```
        use async_event_emitter::AsyncEventEmitter as EventEmitter;
        use serde::{Deserialize, Serialize};
        #[tokio::main]
        async fn main () {
        let event_emitter = EventEmitter::new();
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

        ```
        use async_event_emitter::AsyncEventEmitter as EventEmitter;
        let event_emitter = EventEmitter::new();

        let listener_id = event_emitter.on("Hello", |_: ()|  async {println!("Hello World")});
        match event_emitter.remove_listener(listener_id) {
            Some(listener_id) => print!("Removed event listener!"),
            None => print!("No event listener of that id exists")
        }
        ```

        Listening to all emitted events with a single listener

        ```rust
        use async_event_emitter::AsyncEventEmitter as EventEmitter;
         #[tokio::main]
          async fn main() {
         let mut event_emitter = EventEmitter::new();
         // this will print Hello world two because of
         event_emitter.on_all(|value: i32| async move { println!("Hello world! - {value}") });
         // >> "Hello world! - 1"
         // >> "Hello world! - 2"
         event_emitter.emit("Some event", 1).await;
         event_emitter.emit("next event", 2).await;
      }

        ```
        ## Creating a Global EventEmitter

        It's likely that you'll want to have a single EventEmitter instance that can be shared across files;

        After all, one of the main points of using an EventEmitter is to avoid passing down a value through several nested functions/types and having a global subscription service.

        ```
        // global_event_emitter.rs
        use lazy_static::lazy_static;
        use async_event_emitter::AsyncEventEmitter;

        // Use lazy_static! because the size of EventEmitter is not known at compile time
        lazy_static! {
            // Export the emitter with `pub` keyword
            pub static ref EVENT_EMITTER: AsyncEventEmitter = AsyncEventEmitter::new();
        }

        #[tokio::main]
        async fn main() {
            EVENT_EMITTER.on("Hello", |_:()|  async {println!("hello there!")});
            EVENT_EMITTER.emit("Hello", ()).await;
        }

        async fn random_function() {
            // When the <"Hello"> event is emitted in main.rs then print <"Random stuff!">
            EVENT_EMITTER.on("Hello", |_: ()| async { println!("Random stuff!")});
        }
        ```
     ### Usage with other runtimes
     Check out the examples from the [typed version of this crate](https://docs.rs/typed-emitter/0.1.2/typed_emitter/#getting-started), just replace the emntter type.

     ### Testing
       Run the tests on this crate with all-features enabled as follows:
       ``` cargo test --all-features```


        License: MIT
*/

use arc_swap::{ArcSwap, ArcSwapOption};
use futures::future::{join_all, BoxFuture, Future, FutureExt};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
pub type AsyncCB = dyn Fn(&[u8]) -> BoxFuture<'static, ()> + Send + Sync + 'static;
#[derive(Clone)]
pub struct AsyncListener {
    pub callback: Arc<AsyncCB>,
    pub limit: Option<Arc<AtomicU64>>,
    pub id: Uuid,
}
type Listeners = ArcSwap<Vec<AsyncListener>>;
#[derive(Default)]
struct EmitterInner {
    listeners: papaya::HashMap<String, Listeners>,
    all_listener: ArcSwapOption<AsyncListener>,
}
#[derive(Default, Clone)]
pub struct AsyncEventEmitter {
    inner: Arc<EmitterInner>,
}

fn remove_from(listeners: &Listeners, id: Uuid) -> bool {
    let previous = listeners.rcu(|current| {
        current
            .iter()
            .filter(|listener| listener.id != id)
            .cloned()
            .collect::<Vec<_>>()
    });
    previous.iter().any(|listener| listener.id == id)
}

fn parse<F, T, C>(callback: C) -> Arc<AsyncCB>
where
    for<'de> T: Deserialize<'de>,
    C: Fn(T) -> F + Send + Sync + 'static,
    F: Future<Output = ()> + Send + Sync + 'static,
{
    Arc::new(move |bytes: &[u8]| {
        let value: T = bincode::deserialize(bytes).unwrap_or_else(|_| {
            panic!(
                " value can't be deserialized into type {}",
                std::any::type_name::<T>()
            )
        });
        callback(value).boxed()
    })
}

impl AsyncEventEmitter {
    pub fn new() -> Self {
        Self::default()
    }
    /// Returns the numbers of events
    /// # Example
    ///
    /// ```rust
    /// use async_event_emitter::AsyncEventEmitter;
    /// let event_emitter = AsyncEventEmitter::new();
    /// event_emitter.event_count(); // returns  0
    /// ```
    pub fn event_count(&self) -> usize {
        self.inner.listeners.len()
    }

    /// Returns all listeners on the specified event
    /// # Example
    /// ```rust
    /// use async_event_emitter::AsyncEventEmitter;
    /// #[tokio::main]
    /// async fn main() {
    ///     let emitter = AsyncEventEmitter::new();
    ///     emitter.on("test", |value: ()| async { println!("Hello world!") });
    ///     emitter.emit("test", ()).await;
    ///     let listeners = emitter.listeners_by_event("test");
    ///     println!("{listeners:?}");
    /// }
    /// ```
    pub fn listeners_by_event(&self, event: &str) -> Vec<AsyncListener> {
        self.inner
            .listeners
            .pin()
            .get(event)
            .map(|listeners| listeners.load().to_vec())
            .unwrap_or_default()
    }
    /// Returns the numbers of listners per event
    /// # Example
    ///
    /// ```rust
    /// use async_event_emitter::AsyncEventEmitter;
    /// #[tokio::main]
    /// async fn main() {
    ///     let emitter = AsyncEventEmitter::new();
    ///     emitter.on("test", |value: ()| async { println!("Hello world!") });
    ///     emitter.emit("test", ()).await;
    ///     emitter.listener_count_by_event("test"); // returns  1
    /// }
    /// ```
    pub fn listener_count_by_event(&self, event: &str) -> usize {
        self.inner
            .listeners
            .pin()
            .get(event)
            .map_or(0, |listeners| listeners.load().len())
    }

    /// Emits an event of the given parameters and executes each callback that is listening to that event asynchronously by spawning a task for each callback.
    ///
    /// # Example
    ///
    /// ```rust
    /// use async_event_emitter::AsyncEventEmitter;
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let event_emitter = AsyncEventEmitter::new();
    ///
    ///     // Emits the <"Some event"> event and a value <"Hello programmer">
    ///     // The value can be of any type as long as it implements the serde Serialize trait
    ///     event_emitter.emit("Some event", "Hello programmer!").await;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn emit<T>(&self, event: &str, value: T) -> anyhow::Result<()>
    where
        T: Serialize,
    {
        let futures = {
            let events = self.inner.listeners.pin();
            let listeners = events.get(event);
            let snapshot = listeners.map(ArcSwap::load);
            let global = self.inner.all_listener.load();
            if snapshot.as_ref().map_or(true, |current| current.is_empty()) && global.is_none() {
                return Ok(());
            }

            let bytes: Vec<u8> = bincode::serialize(&value)?;
            let mut futures = Vec::new();
            let mut expired = Vec::new();
            for listener in snapshot.iter().flat_map(|current| current.iter()) {
                let claimed = listener.limit.as_ref().map_or(true, |limit| {
                    // `try_update` replaces `fetch_update` only on newer toolchains; keep this for older Rust versions
                    #[allow(deprecated)]
                    let claimed =
                        limit.fetch_update(AtomicOrdering::AcqRel, AtomicOrdering::Acquire, |n| {
                            n.checked_sub(1)
                        });
                    claimed.is_ok()
                });
                if claimed {
                    futures.push((listener.callback)(&bytes));
                } else {
                    expired.push(listener.id);
                }
            }
            if let Some(global_listener) = global.as_ref() {
                futures.push((global_listener.callback)(&bytes));
            }
            if let Some(listeners) = listeners {
                for id in expired {
                    remove_from(listeners, id);
                }
            }
            futures
        };

        join_all(futures).await;
        Ok(())
    }

    /// Removes an event listener with the given id
    ///
    /// # Example
    ///
    /// ```
    /// use async_event_emitter::AsyncEventEmitter;
    /// let event_emitter = AsyncEventEmitter::new();
    /// let listener_id =
    ///     event_emitter.on("Some event", |value: ()| async { println!("Hello world!") });
    /// // Removes the listener that we just added
    /// event_emitter.remove_listener(listener_id);
    /// ```
    pub fn remove_listener(&self, id_to_delete: Uuid) -> Option<Uuid> {
        let removed = self.inner.listeners.pin().values().any(|listeners| {
            listeners
                .load()
                .iter()
                .any(|listener| listener.id == id_to_delete)
                && remove_from(listeners, id_to_delete)
        });
        if removed {
            return Some(id_to_delete);
        }
        let previous = self.inner.all_listener.rcu(|current| match current {
            Some(listener) if listener.id == id_to_delete => None,
            other => other.clone(),
        });
        match previous {
            Some(listener) if listener.id == id_to_delete => Some(id_to_delete),
            _ => None,
        }
    }

    /// Adds an event listener that will only execute the listener x amount of times - Then the listener will be deleted.
    /// Returns the id of the newly added listener.
    ///
    /// # Example
    ///
    /// ```
    /// use async_event_emitter::AsyncEventEmitter;
    /// #[tokio::main]
    /// async fn main() {
    /// let event_emitter = AsyncEventEmitter::new();
    /// // Listener will be executed 3 times. After the third time, the listener will be deleted.
    /// event_emitter.on_limited("Some event", Some(3), |value: ()| async{ println!("Hello world!")});
    /// event_emitter.emit("Some event", ()).await; // 1 >> "Hello world!"
    /// event_emitter.emit("Some event", ()).await; // 2 >> "Hello world!"
    /// event_emitter.emit("Some event", ()).await; // 3 >> "Hello world!"
    /// event_emitter.emit("Some event", ()).await; // 4 >> <Nothing happens here because listener was deleted after the 3rd call>
    /// }
    /// ```
    pub fn on_limited<F, T, C>(&self, event: &str, limit: Option<u64>, callback: C) -> Uuid
    where
        for<'de> T: Deserialize<'de>,
        C: Fn(T) -> F + Send + Sync + 'static,
        F: Future<Output = ()> + Send + Sync + 'static,
    {
        let id = Uuid::new_v4();
        let listener = AsyncListener {
            id,
            limit: limit.map(|num| AtomicU64::new(num).into()),
            callback: parse(callback),
        };

        let events = self.inner.listeners.pin();
        events
            .get(event)
            .unwrap_or_else(|| events.get_or_insert_with(event.to_owned(), Listeners::default))
            .rcu(|current| {
                let mut next = Vec::with_capacity(current.len() + 1);
                next.extend(current.iter().cloned());
                next.push(listener.clone());
                next
            });

        id
    }

    /// Adds an event listener that will only execute the callback once - Then the listener will be deleted.
    /// Returns the id of the newly added listener.
    ///
    /// # Example
    ///
    /// ```rust
    /// use async_event_emitter::AsyncEventEmitter;
    /// let  event_emitter = AsyncEventEmitter::new();
    ///
    /// event_emitter.once("Some event", |value: ()| async {println!("Hello world!")});
    /// event_emitter.emit("Some event", ()); // First event is emitted and the listener's callback is called once
    /// // >> "Hello world!"
    ///
    /// event_emitter.emit("Some event", ());
    /// // >> <Nothing happens here since listener was deleted>
    /// ```
    pub fn once<F, T, C>(&self, event: &str, callback: C) -> Uuid
    where
        for<'de> T: Deserialize<'de>,
        C: Fn(T) -> F + Send + Sync + 'static,
        F: Future<Output = ()> + Send + Sync + 'static,
    {
        self.on_limited(event, Some(1), callback)
    }

    /// Adds an event listener with a callback that will get called whenever the given event is emitted.
    /// Returns the id of the newly added listener.
    ///
    /// # Example
    ///
    /// ```rust
    /// use async_event_emitter::AsyncEventEmitter;
    /// let  event_emitter = AsyncEventEmitter::new();
    ///
    /// // This will print <"Hello world!"> whenever the <"Some event"> event is emitted
    /// // The type of the `value` parameter for the closure MUST be specified and, if you plan to use the `value`, the `value` type
    /// // MUST also match the type that is being emitted (here we just use a throwaway `()` type since we don't care about using the `value`)
    /// event_emitter.on("Some event", |value: ()| async { println!("Hello world!")});
    /// ```
    pub fn on<F, T, C>(&self, event: &str, callback: C) -> Uuid
    where
        for<'de> T: Deserialize<'de>,
        C: Fn(T) -> F + Send + Sync + 'static,
        F: Future<Output = ()> + Send + Sync + 'static,
    {
        self.on_limited(event, None, callback)
    }
    /// Adds an event listener called for whenever every event is called
    /// Returns the id of the newly added listener.
    ///
    /// # Example
    /// ```rust
    /// use async_event_emitter::AsyncEventEmitter;
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut event_emitter = AsyncEventEmitter::new();
    ///     // this will print Hello world two because of
    ///     event_emitter.on_all(|value: ()| async { println!("Hello world!") });
    ///     event_emitter.emit("Some event", ()).await;
    ///     // >> "Hello world!"
    ///
    ///     event_emitter.emit("next event", ()).await;
    ///     // >> <Nothing happens here since listener was deleted>
    /// }
    /// ```
    pub fn on_all<F, T, C>(&self, callback: C) -> Uuid
    where
        for<'de> T: Deserialize<'de>,
        C: Fn(T) -> F + Send + Sync + 'static,
        F: Future<Output = ()> + Send + Sync + 'static,
    {
        let id = Uuid::new_v4();
        let listener = Arc::new(AsyncListener {
            id,
            limit: None,
            callback: parse(callback),
        });

        let previous = self.inner.all_listener.rcu(|current| match current {
            None => Some(Arc::clone(&listener)),
            other => other.clone(),
        });
        assert!(previous.is_none(), "only one global listener is allowed");

        id
    }
}

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::Arc;
impl fmt::Debug for AsyncListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsyncListener")
            .field("id", &self.id)
            .field("limit", &self.limit)
            .finish()
    }
}

impl fmt::Debug for AsyncEventEmitter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsyncEventEmitter")
            .field("listeners", &self.inner.listeners)
            .finish()
    }
}

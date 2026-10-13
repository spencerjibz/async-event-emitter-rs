#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(
    missing_debug_implementations,
    missing_docs,
    clippy::pedantic,
    clippy::nursery
)]

use arc_swap::{ArcSwap, ArcSwapOption};
use futures::future::{join_all, BoxFuture, Future, FutureExt};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
/// Type-erased listener callback: takes the serialized event value and returns a future.
pub type AsyncCB = dyn Fn(&[u8]) -> BoxFuture<'static, ()> + Send + Sync + 'static;
/// A registered event listener.
#[derive(Clone)]
pub struct AsyncListener {
    /// The callback run when the listener's event is emitted.
    pub callback: Arc<AsyncCB>,
    /// Remaining number of calls, or `None` if the listener never expires.
    pub limit: Option<Arc<AtomicU64>>,
    /// Unique id returned when the listener was registered.
    pub id: Uuid,
}
type Listeners = ArcSwap<Vec<AsyncListener>>;
#[derive(Default)]
struct EmitterInner {
    listeners: papaya::HashMap<String, Listeners>,
    all_listener: ArcSwapOption<AsyncListener>,
}
/// An async, lock-free event emitter.
///
/// Clones share the same listeners.
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
    /// Creates an emitter with no listeners.
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn listener_count_by_event(&self, event: &str) -> usize {
        self.inner
            .listeners
            .pin()
            .get(event)
            .map_or(0, |listeners| listeners.load().len())
    }

    /// Emits an event of the given parameters and executes each callback that is listening to that event asynchronously by spawning a task for each callback.
    ///
    /// # Errors
    ///
    /// Returns an error if `value` can't be serialized.
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
    // Callers often remove a listener without checking whether it existed
    #[allow(clippy::must_use_candidate)]
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
    /// # Panics
    ///
    /// The callback panics when an emitted value can't be deserialized into `T`.
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
    /// # Panics
    ///
    /// Panics if a global listener is already registered. The callback panics when an
    /// emitted value can't be deserialized into `T`.
    ///
    /// # Example
    /// ```rust
    /// use async_event_emitter::AsyncEventEmitter;
    /// #[tokio::main]
    /// async fn main() {
    ///     let event_emitter = AsyncEventEmitter::new();
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
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for AsyncEventEmitter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsyncEventEmitter")
            .field("listeners", &self.inner.listeners)
            .finish()
    }
}

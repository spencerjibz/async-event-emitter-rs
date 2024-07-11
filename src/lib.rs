/*!


        an Async implementation of the  [`event-emitter-rs`](https://crates.io/crates/event-emitter-rs) crate

        Allows you to subscribe to events with callbacks and also fire those events.
        Events are in the form of (strings, value) and callbacks are in the form of closures that take in a value parameter;

        ## Differences between this crate and [`event-emitter-rs`](https://crates.io/crates/event-emitter-rs)
        - Emitted values should implement an extra trait (Debug) in addition to Serde's Serialize and Deserialize.
        - This is an async implementation, not limited to tokio, but also supports async-std  under the ```use-async-std``` feature flag.
        - The listener methods ***(on and once)*** take a callback that returns a future instead of a merely a closure.
        - The emit methods executes each callback on each event by spawning a tokio task instead of a std::thread


        ## Getting Started

        ```
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
        ## Basic Usage
        We can emit and listen to values of any type so long as they implement  the Debug trait and serde's Serialize and Deserialize traits.
        A single EventEmitter instance can have listeners to values of multiple types.

        ```
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

        ```
        use async_event_emitter::AsyncEventEmitter as EventEmitter;
        let mut event_emitter = EventEmitter::new();

        let listener_id = event_emitter.on("Hello", |_: ()|  async {println!("Hello World")});
        match event_emitter.remove_listener(&listener_id) {
            Some(listener_id) => print!("Removed event listener!"),
            None => print!("No event listener of that id exists")
        }
        ```
        ## Creating a Global EventEmitter

        It's likely that you'll want to have a single EventEmitter instance that can be shared across files;

        After all, one of the main points of using an EventEmitter is to avoid passing down a value through several nested functions/types and having a global subscription service.

        ```
        // global_event_emitter.rs
        use lazy_static::lazy_static;
        use futures::lock::Mutex;
        use async_event_emitter::AsyncEventEmitter;

        // Use lazy_static! because the size of EventEmitter is not known at compile time
        lazy_static! {
            // Export the emitter with `pub` keyword
            pub static ref EVENT_EMITTER: Mutex<AsyncEventEmitter> = Mutex::new(AsyncEventEmitter::new());
        }

        #[tokio::main]
        async fn main() {
            // We need to maintain a lock through the mutex so we can avoid data races
            EVENT_EMITTER.lock().await.on("Hello", |_:()|  async {println!("hello there!")});
            EVENT_EMITTER.lock().await.emit("Hello", ()).await;
        }

        async fn random_function() {
            // When the <"Hello"> event is emitted in main.rs then print <"Random stuff!">
            EVENT_EMITTER.lock().await.on("Hello", |_: ()| async { println!("Random stuff!")});
        }
        ```
     ## Using async-std instead of tokio
      Tokio is the default  runtime for this library but async-std support can be able enabled by disabling default-features on the crate and enable the ```use-async-std``` feature.
     <br>
      **Note**: Use simply replace tokio::main with async-std::main and tokio::test with async-std::test (provided you've enabled the "attributes" feature on the crate.

     ### Testing
       Run the tests on this crate with all-features enabled as follows:
       ``` cargo test --all-features```


        License: MIT
*/

use futures::future::{BoxFuture, Future, FutureExt};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

pub type AsyncCB = dyn Fn(Vec<u8>) -> BoxFuture<'static, ()> + Send + Sync + 'static;

#[derive(Clone)]
pub struct AsyncListener {
    pub callback: Arc<AsyncCB>,
    pub limit: Option<u64>,
    pub id: String,
}

#[derive(Default, Clone)]
pub struct AsyncEventEmitter {
    pub listeners: HashMap<String, Vec<AsyncListener>>,
}

impl AsyncEventEmitter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Emits an event of the given parameters and executes each callback that is listening to that event asynchronously by spawning a task for each callback.
    ///
    /// # Example
    ///
    /// ```
    /// use async_event_emitter::AsyncEventEmitter;
    /// let mut event_emitter = AsyncEventEmitter::new();
    ///
    /// // Emits the <"Some event"> event and a value <"Hello programmer">
    /// // The value can be of any type as long as it implements the serde Serialize trait
    /// tokio_test::block_on(async {
    ///     event_emitter.emit("Some event", "Hello programmer!").await;
    /// })
    /// ```

    pub async fn emit<'a, T>(&mut self, event: &str, value: T) -> anyhow::Result<()>
    where
        T: Serialize + Deserialize<'a> + Send + Sync + 'a + std::fmt::Debug,
    {
        #[cfg(feature = "use-async-std")]
        use async_std::task::spawn;
        #[cfg(not(feature = "use-async-std"))]
        use tokio::spawn;

        let mut callback_handlers: Vec<_> = Vec::new();

        if let Some(listeners) = self.listeners.get_mut(event) {
            let mut listeners_to_remove: Vec<usize> = Vec::new();
            for (index, listener) in listeners.iter_mut().enumerate() {
                let bytes: Vec<u8> = bincode::serialize(&value)?;

                let callback = Arc::clone(&listener.callback);

                match listener.limit {
                    None => {
                        callback_handlers.push(spawn(async move { callback(bytes).await }));
                    }
                    Some(limit) => {
                        if limit != 0 {
                            callback_handlers.push(spawn(async move { callback(bytes).await }));

                            listener.limit = Some(limit - 1);
                        } else {
                            listeners_to_remove.push(index);
                        }
                    }
                }
            }

            // Reverse here so we don't mess up the ordering of the vector
            for index in listeners_to_remove.into_iter().rev() {
                listeners.remove(index);
            }
        }

        for handles in callback_handlers {
            _ = handles.await;
        }

        Ok(())
    }

    /// Removes an event listener with the given id
    ///
    /// # Example
    ///
    /// ```
    /// use async_event_emitter::AsyncEventEmitter;
    /// let mut event_emitter = AsyncEventEmitter::new();
    /// let listener_id =
    ///     event_emitter.on("Some event", |value: ()| async { println!("Hello world!") });
    /// println!("{:?}", event_emitter.listeners);
    ///
    /// // Removes the listener that we just added
    /// event_emitter.remove_listener(&listener_id);
    /// ```

    pub fn remove_listener(&mut self, id_to_delete: &str) -> Option<String> {
        for (_, event_listeners) in self.listeners.iter_mut() {
            if let Some(index) = event_listeners
                .iter()
                .position(|listener| listener.id == id_to_delete)
            {
                event_listeners.remove(index);
                return Some(id_to_delete.to_string());
            }
        }

        None
    }

    /// Adds an event listener that will only execute the listener x amount of times - Then the listener will be deleted.
    /// Returns the id of the newly added listener.
    ///
    /// # Example
    ///
    /// ```
    /// use async_event_emitter::AsyncEventEmitter;
    /// let mut event_emitter = AsyncEventEmitter::new();
    ///
    /// // Listener will be executed 3 times. After the third time, the listener will be deleted.
    ///
    /// event_emitter.on_limited("Some event", Some(3), |value: ()| async{ println!("Hello world!")});
    /// tokio_test::block_on( async{
    /// event_emitter.emit("Some event", ()).await; // 1 >> "Hello world!"
    /// event_emitter.emit("Some event", ()).await; // 2 >> "Hello world!"
    /// event_emitter.emit("Some event", ()).await; // 3 >> "Hello world!"
    /// event_emitter.emit("Some event", ()).await; // 4 >> <Nothing happens here because listener was deleted after the 3rd call>
    ///
    /// });
    /// ```

    pub fn on_limited<F, T, C>(&mut self, event: &str, limit: Option<u64>, callback: C) -> String
    where
        for<'de> T: Deserialize<'de> + std::fmt::Debug,
        C: Fn(T) -> F + Send + Sync + 'static,
        F: Future<Output = ()> + Send + Sync + 'static,
    {
        let id = Uuid::new_v4().to_string();
        let parsed_callback = move |bytes: Vec<u8>| {
            let value: T = bincode::deserialize(&bytes).unwrap_or_else(|_| {
                panic!(
                    " value can't be deserialized into type {}",
                    std::any::type_name::<T>()
                )
            });

            callback(value).boxed()
        };

        let listener = AsyncListener {
            id: id.clone(),
            limit,
            callback: Arc::new(parsed_callback),
        };

        match self.listeners.get_mut(event) {
            Some(callbacks) => {
                callbacks.push(listener);
            }
            None => {
                self.listeners.insert(event.to_string(), vec![listener]);
            }
        }

        id
    }

    /// Adds an event listener that will only execute the callback once - Then the listener will be deleted.
    /// Returns the id of the newly added listener.
    ///
    /// # Example
    ///
    /// ```
    /// use async_event_emitter::AsyncEventEmitter;
    /// let mut event_emitter = AsyncEventEmitter::new();
    ///
    /// event_emitter.once("Some event", |value: ()| async {println!("Hello world!")});
    /// event_emitter.emit("Some event", ()); // First event is emitted and the listener's callback is called once
    /// // >> "Hello world!"
    ///
    /// event_emitter.emit("Some event", ());
    /// // >> <Nothing happens here since listener was deleted>
    /// ```
    pub fn once<F, T, C>(&mut self, event: &str, callback: C) -> String
    where
        for<'de> T: Deserialize<'de> + std::fmt::Debug,
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
    /// ```
    /// use async_event_emitter::AsyncEventEmitter;
    /// let mut event_emitter = AsyncEventEmitter::new();
    ///
    /// // This will print <"Hello world!"> whenever the <"Some event"> event is emitted
    /// // The type of the `value` parameter for the closure MUST be specified and, if you plan to use the `value`, the `value` type
    /// // MUST also match the type that is being emitted (here we just use a throwaway `()` type since we don't care about using the `value`)
    /// event_emitter.on("Some event", |value: ()| async { println!("Hello world!")});
    /// ```
    pub fn on<F, T, C>(&mut self, event: &str, callback: C) -> String
    where
        for<'de> T: Deserialize<'de> + std::fmt::Debug,
        C: Fn(T) -> F + Send + Sync + 'static,
        F: Future<Output = ()> + Send + Sync + 'static,
    {
        self.on_limited(event, None, callback)
    }
}

// test the AsyncEventEmitter
// implement fmt::Debug for AsyncEventListener
use std::fmt;
impl fmt::Debug for AsyncListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsyncListener")
            .field("id", &self.id)
            .field("limit", &self.limit)
            .finish()
    }
}

// implement fmt::Debug   for AsyncEventEmitter
impl fmt::Debug for AsyncEventEmitter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsyncEventEmitter")
            .field("listeners", &self.listeners)
            .finish()
    }
}

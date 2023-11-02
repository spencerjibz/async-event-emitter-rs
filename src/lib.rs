use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::task::{self};

use futures::future::{BoxFuture, Future, FutureExt};
use uuid::Uuid;

pub type AsyncCB = dyn Fn(Vec<u8>) -> BoxFuture<'static, ()> + Send + Sync + 'static;

#[derive(Clone)]
pub struct AsyncListener {
    callback: Arc<AsyncCB>,
    limit: Option<u64>,
    id: String,
}

#[derive(Default, Clone)]
pub struct AsyncEventEmitter {
    pub listeners: HashMap<String, Vec<AsyncListener>>,
}

impl AsyncEventEmitter {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn emit<'a, T>(&mut self, event: &str, value: T) -> anyhow::Result<()>
    where
        T: Serialize + Deserialize<'a> + Send + Sync + 'a + std::fmt::Debug,
    {
        let mut callback_handlers: Vec<_> = Vec::new();

        if let Some(listeners) = self.listeners.get_mut(event) {
            let mut listeners_to_remove: Vec<usize> = Vec::new();
            for (index, listener) in listeners.iter_mut().enumerate() {
                let bytes: Vec<u8> = bincode::serialize(&value).unwrap();

                let callback = Arc::clone(&listener.callback);

                match listener.limit {
                    None => {
                        callback_handlers.push(task::spawn(async move {
                            callback(bytes).await;
                        }));
                    }
                    Some(limit) => {
                        if limit != 0 {
                            callback_handlers
                                .push(task::spawn(async move { callback(bytes).await }));
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
            handles.await?;
        }

        Ok(())
    }

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

    fn on_limited<F, T, C>(&mut self, event: &str, limit: Option<u64>, callback: C) -> String
    where
        for<'de> T: Deserialize<'de> + std::fmt::Debug,
        C: Fn(T) -> F + Send + Sync + 'static,
        F: Future<Output = ()> + Send + Sync + 'static,
    {
        let id = Uuid::new_v4().to_string();
        let parsed_callback = move |bytes: Vec<u8>| {
            let value: T = bincode::deserialize(&bytes).unwrap();

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
    pub fn once<F, T, C>(&mut self, event: &str, callback: C) -> String
    where
        for<'de> T: Deserialize<'de> + std::fmt::Debug,
        C: Fn(T) -> F + Send + Sync + 'static,
        F: Future<Output = ()> + Send + Sync + 'static,
    {
        self.on_limited(event, Some(1), callback)
    }
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

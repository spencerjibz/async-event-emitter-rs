use async_event_emitter::AsyncEventEmitter;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use serde::Deserialize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

const BATCH: usize = 1000;
const TASKS: usize = 8;
const EMITS_PER_TASK: usize = 200;

fn runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(TASKS)
        .build()
        .unwrap()
}

fn group_name(base: &str, global: bool) -> String {
    if global {
        format!("{base}+global")
    } else {
        base.to_string()
    }
}

fn build_emitter<T>(
    listeners: usize,
    global: bool,
    counter: &Arc<AtomicUsize>,
) -> Arc<AsyncEventEmitter>
where
    T: for<'de> Deserialize<'de> + 'static,
{
    let emitter = AsyncEventEmitter::new();
    for _ in 0..listeners {
        let counter = counter.clone();
        emitter.on("event", move |_: T| {
            counter.fetch_add(1, Ordering::Relaxed);
            async {}
        });
    }
    if global {
        let counter = counter.clone();
        emitter.on_all(move |_: T| {
            counter.fetch_add(1, Ordering::Relaxed);
            async {}
        });
    }
    Arc::new(emitter)
}

fn bench(c: &mut Criterion, rt: &Runtime, global: bool) {
    let counter = Arc::new(AtomicUsize::new(0));

    let mut group = c.benchmark_group(group_name("emit_small", global));
    group.throughput(Throughput::Elements(BATCH as u64));
    for listeners in [1, 10, 100] {
        let emitter = build_emitter::<u64>(listeners, global, &counter);
        group.bench_with_input(
            BenchmarkId::from_parameter(listeners),
            &listeners,
            |b, _| {
                b.to_async(rt).iter(|| async {
                    for _ in 0..BATCH {
                        emitter.emit("event", 42u64).await.unwrap();
                    }
                });
            },
        );
    }
    group.finish();

    let mut group = c.benchmark_group(group_name("emit_4KiB", global));
    group.throughput(Throughput::Elements(BATCH as u64));
    let emitter = build_emitter::<Vec<u8>>(10, global, &counter);
    let payload = vec![7u8; 4096];
    group.bench_function("10", |b| {
        b.to_async(rt).iter(|| async {
            for _ in 0..BATCH {
                emitter.emit("event", payload.clone()).await.unwrap();
            }
        });
    });
    group.finish();

    let mut group = c.benchmark_group(group_name("on_then_remove", global));
    let emitter = build_emitter::<u64>(0, global, &counter);
    for i in 0..100 {
        emitter.on(&format!("event{}", i % 10), |_: u64| async {});
    }
    group.bench_function("100", |b| {
        b.iter(|| {
            let id = emitter.on("event5", |_: u64| async {});
            emitter.remove_listener(id)
        });
    });
    group.finish();

    let mut group = c.benchmark_group(group_name("concurrent_emit", global));
    group.throughput(Throughput::Elements((TASKS * EMITS_PER_TASK) as u64));
    let emitter = build_emitter::<u64>(10, global, &counter);
    group.bench_function("8_tasks", |b| {
        b.to_async(rt).iter(|| {
            let emitter = emitter.clone();
            async move {
                let tasks: Vec<_> = (0..TASKS)
                    .map(|_| {
                        let emitter = emitter.clone();
                        tokio::spawn(async move {
                            for _ in 0..EMITS_PER_TASK {
                                emitter.emit("event", 1u64).await.unwrap();
                            }
                        })
                    })
                    .collect();
                futures::future::join_all(tasks).await;
            }
        });
    });
    group.finish();

    let mut group = c.benchmark_group(group_name("concurrent_emit_and_register", global));
    group.throughput(Throughput::Elements((TASKS / 2 * EMITS_PER_TASK) as u64));
    let emitter = build_emitter::<u64>(10, global, &counter);
    group.bench_function("8_tasks", |b| {
        b.to_async(rt).iter(|| {
            let emitter = emitter.clone();
            async move {
                let tasks: Vec<_> = (0..TASKS)
                    .map(|task| {
                        let emitter = emitter.clone();
                        tokio::spawn(async move {
                            for _ in 0..EMITS_PER_TASK {
                                if task % 2 == 0 {
                                    emitter.emit("event", 1u64).await.unwrap();
                                } else {
                                    let id = emitter.on("event", |_: u64| async {});
                                    emitter.remove_listener(id);
                                }
                            }
                        })
                    })
                    .collect();
                futures::future::join_all(tasks).await;
            }
        });
    });
    group.finish();
}

fn benches(c: &mut Criterion) {
    let rt = runtime();
    bench(c, &rt, false);
    bench(c, &rt, true);
}

criterion_group! {
    name = emitter_benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .sample_size(20);
    targets = benches
}
criterion_main!(emitter_benches);

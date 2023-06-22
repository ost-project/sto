mod word_builder;

use crate::word_builder::generate_test_strings;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use sto::{Repository, ScopedSto};

fn bench_create(c: &mut Criterion) {
    c.bench_function("create_repository", |b| {
        b.iter_with_large_drop(|| Repository::new())
    });
}

fn bench_single_thread(c: &mut Criterion) {
    const SIZES: &[usize] = &[100, 10_000, 1_000_000];
    let inputs = generate_test_strings(*SIZES.last().unwrap(), 8);

    let mut bg = c.benchmark_group("single_thread");

    SIZES.iter().for_each(|&size| {
        bg.bench_with_input(format!("insert_{size}"), &inputs[..size], |b, words| {
            b.iter_with_large_drop(|| {
                let repo = Repository::new();
                words.iter().for_each(|word| {
                    black_box(ScopedSto::intern_in(word, &repo));
                });
                repo
            });
        });
    });

    SIZES.iter().for_each(|&size| {
        bg.bench_with_input(
            format!("insert_and_duplicate_{size}"),
            &inputs[..size],
            |b, words| {
                b.iter_with_large_drop(|| {
                    let repo = Repository::new();
                    words.iter().for_each(|word| {
                        black_box(ScopedSto::intern_in(word, &repo));
                    });
                    words.iter().for_each(|word| {
                        black_box(ScopedSto::intern_in(word, &repo));
                    });
                    repo
                });
            },
        );
    });
}

fn bench_multi_thread(c: &mut Criterion) {
    const TOTAL_STRINGS: usize = 120_000;
    const THREADS: &[usize] = &[4, 8, 12];
    let inputs = Arc::new(generate_test_strings(TOTAL_STRINGS, 8));

    let mut bg = c.benchmark_group("multi_thread");

    bg.bench_function("insert_non_overlapped_baseline", |b| {
        b.iter_with_large_drop(|| {
            let repo = Repository::new();
            for s in inputs.iter() {
                black_box(ScopedSto::intern_in(s, &repo));
            }
            repo
        })
    });

    THREADS.iter().for_each(|&threads| {
        bg.bench_function(format!("insert_non_overlapped_{threads}"), |b| {
            b.iter_with_large_drop(|| {
                let repo = Repository::new();
                let repo_ref = &repo;
                std::thread::scope(|scope| {
                    for t in 0..threads {
                        let inputs = inputs.clone();
                        scope.spawn(move || {
                            // logically the `take` does not affect the number of iterations,
                            // but for safety reasons, it is added here
                            for s in inputs
                                .iter()
                                .skip(t)
                                .step_by(threads)
                                .take(TOTAL_STRINGS / threads)
                            {
                                black_box(ScopedSto::intern_in(s, repo_ref));
                            }
                        });
                    }
                });
                repo
            })
        });
    });

    THREADS.iter().for_each(|&threads| {
        bg.bench_function(format!("insert_overlapped_{threads}"), |b| {
            b.iter_with_large_drop(|| {
                let repo = Repository::new();
                let repo_ref = &repo;
                std::thread::scope(|scope| {
                    for t in 0..threads {
                        let inputs = inputs.clone();
                        scope.spawn(move || {
                            for s in inputs
                                .iter()
                                .cycle()
                                .skip(t * 1000)
                                .take(TOTAL_STRINGS / threads)
                            {
                                black_box(ScopedSto::intern_in(s, repo_ref));
                            }
                        });
                    }
                });
                repo
            })
        });
    });
}

criterion_group!(create, bench_create);
criterion_group!(single_thread, bench_single_thread);
criterion_group!(multi_thread, bench_multi_thread);
criterion_main!(create, single_thread, multi_thread);

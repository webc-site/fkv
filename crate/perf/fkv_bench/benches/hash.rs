use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use gxhash::gxhash64;

fn hash_bench(c: &mut Criterion) {
  let mut group = c.benchmark_group("gxhash");
  for size in [64usize, 1024, 4096, 65536] {
    let data = vec![0u8; size];
    group.throughput(Throughput::Bytes(size as u64));
    group.bench_function(format!("{size}B"), |b| {
      b.iter(|| gxhash64(black_box(&data), 0))
    });
  }
  group.finish();
}

criterion_group!(benches, hash_bench);
criterion_main!(benches);

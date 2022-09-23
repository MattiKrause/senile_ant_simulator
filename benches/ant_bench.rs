use ant_sim::ant_sim::{AntVisualRangeBuffer};
use ant_sim::ant_sim_frame::{AntPosition, AntSim};
use ant_sim::ant_sim_frame_impl::AntSimVecImpl;

use criterion::{Bencher, BenchmarkId, black_box, Criterion, criterion_group, criterion_main};

type TestSim = AntSimVecImpl;
type TestPos = <TestSim as AntSim>::Position;

fn neighbors_bench(b: &mut Bencher, range: usize, with: impl Fn(TestPos, &TestSim, &mut [&mut [Option<TestPos>]])) {
    let sim = TestSim::new(300, 300).unwrap();
    let mut range_backing_buf: AntVisualRangeBuffer<TestSim> = AntVisualRangeBuffer::new(range);
    let mut range_buf = Vec::with_capacity(range);
    for _ in 0..range {
        range_buf.push([].as_mut_slice())
    }
    range_backing_buf.buffers(range_buf.as_mut_slice());
    b.iter(|| {
        with(black_box(sim.encode(AntPosition { x: 150, y: 150 }).unwrap()), black_box(&sim), black_box(range_buf.as_mut_slice()));
        with(black_box(sim.encode(AntPosition { x: 0, y: 150 }).unwrap()), black_box(&sim), black_box(range_buf.as_mut_slice()));
        with(black_box(sim.encode(AntPosition { x: 0, y: 0 }).unwrap()), black_box(&sim), black_box(range_buf.as_mut_slice()));
        with(black_box(sim.encode(AntPosition { x: 1, y: 2 }).unwrap()), black_box(&sim), black_box(range_buf.as_mut_slice()));
        with(black_box(sim.encode(AntPosition { x: 1, y: 5 }).unwrap()), black_box(&sim), black_box(range_buf.as_mut_slice()));
        with(black_box(sim.encode(AntPosition { x: 3, y: 1 }).unwrap()), black_box(&sim), black_box(range_buf.as_mut_slice()));
        with(black_box(sim.encode(AntPosition { x: 299, y: 0 }).unwrap()), black_box(&sim), black_box(range_buf.as_mut_slice()));
        with(black_box(sim.encode(AntPosition { x: 299, y: 295 }).unwrap()), black_box(&sim), black_box(range_buf.as_mut_slice()));
        with(black_box(sim.encode(AntPosition { x: 295, y: 295 }).unwrap()), black_box(&sim), black_box(range_buf.as_mut_slice()));
    })
}

fn bench_neighbors(c: &mut Criterion) {
    let mut group = c.benchmark_group("bench-normal");
    for r in 1..=7 {
        group.bench_function(BenchmarkId::new("range", r), |b| {
            neighbors_bench(b, r, |pos, sim, b| {
                ant_sim::ant_sim::neighbors(sim, &pos, b);
            })
        });
    }
    group.finish();
    let mut group = c.benchmark_group("bench-unsafe");
    for r in 1..=7 {
        group.bench_function(BenchmarkId::new("range", r), |b| {
            neighbors_bench(b, r, |pos, sim, b| {
                ant_sim::ant_sim::neighbors_unsafe(sim, &pos, b);
            })
        });
    }
    group.finish();
}

criterion_group!(neighbors, bench_neighbors);
criterion_main!(neighbors);
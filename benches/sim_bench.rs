use criterion::{BatchSize, BenchmarkGroup, Criterion, criterion_group, criterion_main};
use criterion::measurement::Measurement;
use rand::{Rng, RngCore};
use ant_sim::ant_sim::{AntSimConfig, AntSimulator, AntVisualRangeBuffer};
use ant_sim::ant_sim_ant::{Ant, AntState};
use ant_sim::ant_sim_frame::{AntPosition, AntSim};
use ant_sim::ant_sim_frame_impl::AntSimVecImpl;


static POINTS_R1: [(f64, f64); 8] = [
    (1.0, 0.0),
    (std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
    (0.0, 1.0),
    (-std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
    (-1.0, 0.0),
    (-std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2),
    (-0.0, -1.0),
    (std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2),
];

fn bench_large<A: AntSim>(new: impl FnOnce(usize, usize) -> Option<A>) -> Option<AntSimulator<A>> {
    let sim = new(10000, 10000)?;
    let mid = sim.encode(AntPosition { x: 5000, y: 5000 })?;
    let mut rng = rand::prelude::thread_rng();
    let ants = (0..100).map(|_| Ant {
        position: mid.clone(),
        last_position: mid.clone(),
        state: AntState::Foraging,
        explore_weight: rng.gen_range(0.0..2.0)
    }).collect::<Vec<_>>();
    let ant_sim = AntSimulator {
        sim,
        ants,
        seed: rng.next_u64(),
        config: AntSimConfig {
            distance_points: Box::new(POINTS_R1),
            food_haul_amount: 255,
            pheromone_decay_amount: 255,
            seed_step: 100,
            visual_range: AntVisualRangeBuffer::new(5)
        }
    };
    Some(ant_sim)
}

fn bench_impl<A: AntSim, M: Measurement>(group: &mut BenchmarkGroup<M>, new: impl FnOnce(usize, usize) -> Option<A> + Clone)
    where AntSimulator<A>: Clone
{
    let sim= bench_large(new.clone());
    if let Some(sim) = sim {
        group.bench_function("large board", |bencher| {
            bencher.iter_batched(|| (sim.clone(), sim.clone()), |(mut sa, mut sb)| {
                let mut a = &mut sa;
                let mut b = &mut sb;
                for _ in 0..10000 {
                    a.update(b);
                    std::mem::swap(&mut a, &mut b)
                }
            }, BatchSize::LargeInput)
        });
    }
}

fn bench_vec_impl(bencher: &mut Criterion) {
    let mut group = bencher.benchmark_group("ant-sim-vec-impl");
    bench_impl(&mut group, |w, h| AntSimVecImpl::new(w, h).ok());
}

criterion_group!(bench_sims, bench_vec_impl);
criterion_main!(bench_sims);
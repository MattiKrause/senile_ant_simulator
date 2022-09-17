use crate::{Ant, AntSim, AntSimCell, AntState};
use crate::ant_sim_frame::NonMaxU8;

#[derive(Clone)]
pub struct AntSimulator<A: AntSim> {
    pub sim: A,
    pub ants: Vec<Ant<A>>,
    pub seed: u64,
    pub decay_step: u8,
    pub config: AntSimConfig
}

#[derive(Clone, Debug)]
pub struct AntSimConfig {
    pub distance_points: Box<[(f64, f64); 8]>,
    pub haul_amount: u8,
    pub decay_rate: u8,
}

//calculated using the equidistant_points function, but as of yet, rust does not support const floating point math
static _POINTS: [(f64, f64); 8] = [
    (1.0, 0.0),
    (std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
    (0.0, 1.0),
    (-std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
    (-1.0, 0.0),
    (-std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2),
    (-0.0, -1.0),
    (std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2),
];
/*
const fn equidistant_points<const N: usize>() -> [(f64, f64); N] {
    let mut res = [(0.0,0.0); N];
    let mut p = 0;
    let angle_diff = (2.0 * std::f64::consts::PI) / (N as f64);
    while p < N {
        let angle = angle_diff * p as f64;
        res[p] = (angle.cos(), angle.sin());
    }
    res
}
*/

impl<A: AntSim> AntSimulator<A> {
    pub fn update(&self, update_into: &mut AntSimulator<A>) {
        assert!(self.sim.check_compatible(&update_into.sim));
        update_into.ants.clone_from_slice(&self.ants);
        Self::decay_pheromones(&self.sim, &mut update_into.sim, self.decay_step, self.config.decay_rate);
        self.update_ants(&mut update_into.ants, &mut update_into.sim);
        Self::update_ant_trail(&self.ants, &mut update_into.sim);
        update_into.decay_step = self.decay_step;
    }
    fn update_ants(&self, ants: &mut [Ant<A>], update_into: &mut A) {
        fn take_food(amount: u8, haul_amount: u8) -> (u8, AntSimCell) {
            if amount > haul_amount {
                (haul_amount, AntSimCell::Food { amount: amount - haul_amount })
            } else {
                (amount, AntSimCell::Path { pheromone_food: NonMaxU8::new(0), pheromone_home: NonMaxU8::new(0) })
            }
        }
        for (i, ant) in ants.iter_mut().enumerate() {
            let state = ant.state_mut().clone();
            match (self.sim.cell(ant.position()).unwrap(), state) {
                (AntSimCell::Food { amount }, AntState::Foraging) => {
                    let (haul_amount, new_cell) = take_food(amount, self.config.haul_amount);
                    *ant.state_mut() = AntState::Hauling { amount: haul_amount };
                    update_into.set_cell(ant.position(), new_cell);
                }
                (AntSimCell::Home, AntState::Hauling { .. }) => {
                    *ant.state_mut() = AntState::Foraging;
                }
                _ => {
                    let seed = self.seed + i as u64;
                    ant.move_to_next(seed, self.config.distance_points.as_ref(), &self.sim);
                }
            }
        }
    }
    fn decay_pheromones(from: &A, on_sim: &mut A, mut decay_step: u8, decay_rate: u8) {
        fn decay_path(p_food: NonMaxU8, p_home: NonMaxU8, decay_by: u8) -> AntSimCell {
            AntSimCell::Path {
                pheromone_food: p_food.dec_by(decay_by),
                pheromone_home: p_home.dec_by(decay_by),
            }
        }
        from.cells()
            .map(|(cell, pos): (AntSimCell, A::Position)| {
                decay_step = (decay_step + 1) % decay_rate;
                match cell {
                    AntSimCell::Path { pheromone_food, pheromone_home } => {
                        let decay_by = if decay_step == 0 { 1 } else { 0 };
                        let cell = decay_path(pheromone_food, pheromone_home, decay_by);
                        (cell, pos)
                    }
                    other => (other, pos)
                }
            })
            .for_each(|(cell, pos)| {
                on_sim.set_cell(&pos, cell);
            });
    }
    fn update_ant_trail(old_ants: &[Ant<A>], update_into: &mut A) {
        for ant in old_ants {
            let cell = update_into.cell(ant.position()).unwrap();
            let new_cell = match cell {
                AntSimCell::Path { pheromone_food, pheromone_home } => {
                    match ant.state() {
                        AntState::Foraging => {
                            AntSimCell::Path { pheromone_food, pheromone_home: NonMaxU8::new(u8::MAX - 1) }
                        }
                        AntState::Hauling { .. } => {
                            AntSimCell::Path { pheromone_food: NonMaxU8::new(u8::MAX - 1), pheromone_home }
                        }
                    }
                }
                old => old
            };
            update_into.set_cell(ant.position(), new_cell);
        }
    }
}
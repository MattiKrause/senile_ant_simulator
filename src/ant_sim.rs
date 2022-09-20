use std::cmp::min;
use crate::{Ant, AntPosition, AntSim, AntSimCell, AntState};
use crate::ant_sim_frame::NonMaxU8;

/// Contains the context of a game execution
#[derive(Clone)]
pub struct AntSimulator<A: AntSim> {
    pub sim: A,
    pub ants: Vec<Ant<A>>,
    pub seed: u64,
    pub decay_step: u8,
    pub config: AntSimConfig<A>
}

/// The Configuration of a simulation, this should not change over the course of the game
#[derive(Clone)]
pub struct AntSimConfig<A: AntSim + ?Sized> {
    /// The ant should prioritise fields in the opposite direction of where it came from.
    /// In order to achieve that, all directions all mapped to a point from the array, then
    /// the overall score of the direction is weighted with distance between the point of the
    /// direction and the point of the direction from which the ant came.
    ///
    /// To support that strategy optimally, the points should be laid out in a circle with equal
    /// distance between them. They should appear in clockwise order. To change weighing,
    /// a circle with a different radius can be used
    pub distance_points: Box<[(f64, f64); 8]>,
    /// The amount on ant takes from one food source
    pub food_haul_amount: u8,
    /// The rate in which pheromones decay, will decay every 1 tick if set to 1, every 2 ticks when set to 2, etc.
    pub pheromone_decay_rate: u8,
    /// The rate at which the seed advances
    pub seed_step: u64,
    pub visual_range: AntVisualRangeBuffer<A>
}

#[derive(Clone, Debug)]
pub struct AntVisualRangeBuffer<A: AntSim + ?Sized> {
    backing: Box<[Option<A::Position>]>,
    range: usize
}

impl <A: AntSim + ?Sized> AntVisualRangeBuffer<A> {
    pub fn new(range: usize) -> Self {
        Self {
            backing: vec![None; Self::expected_size(range)].into_boxed_slice(),
            range
        }
    }
    pub fn range(&self) -> usize {
        self.range
    }
    pub fn buffers<'a>(&'a mut self, write_into: &mut [&'a mut [Option<A::Position>]]) {
        assert!(self.backing.len() >= Self::expected_size(self.range));
        assert!(write_into.len() <= self.range);
        let mut rem = self.backing.as_mut();
        for r in 0..write_into.len() {
            let buf_size = (r + 1) * 8;
            (write_into[r], rem) = rem.split_at_mut(buf_size);
        }
    }
    fn expected_size(range: usize) -> usize {
        ((range * (range + 1)) / 2) * 8
    }
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
        let mut visual_buffer = Vec::with_capacity(update_into.config.visual_range.range());
        for _ in 0..update_into.config.visual_range.range() {
            visual_buffer.push([].as_mut_slice());
        }
        update_into.config.visual_range.buffers(&mut visual_buffer);
        Self::decay_pheromones(&self.sim, &mut update_into.sim, self.decay_step, self.config.pheromone_decay_rate);
        self.update_ants(&mut update_into.ants, &mut update_into.sim, &mut visual_buffer);
        Self::update_ant_trail(&self.ants, &mut update_into.sim);
        update_into.decay_step = (self.decay_step + 1) % self.config.pheromone_decay_rate;
        update_into.seed = self.seed.wrapping_add(self.config.seed_step);
    }

    /// Updates the ant agents:
    /// * if they found food(are standing on a food pixel), take food and set state to Hauling
    /// * if they brought food to the hive(are standing on a home pixel while in Hauling state),
    /// set them to foraging
    /// * otherwise, they try to find their objective, given  by their current state
    fn update_ants(&self, ants: &mut [Ant<A>], update_into: &mut A, visual_buffer: &mut [&mut [Option<A::Position>]]) {
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
                    let (haul_amount, new_cell) = take_food(amount, self.config.food_haul_amount);
                    *ant.state_mut() = AntState::Hauling { amount: haul_amount };
                    ant.stand_still();
                    update_into.set_cell(ant.position(), new_cell);
                }
                (AntSimCell::Home, AntState::Hauling { .. }) => {
                    ant.stand_still();
                    *ant.state_mut() = AntState::Foraging;
                }
                _ => {
                    let seed = self.seed + i as u64;
                    ant.move_to_next2::<fasthash::mum::Hasher64>(seed, self.config.distance_points.as_ref(), &self.sim, visual_buffer);
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

pub fn neighbors<A: AntSim + ?Sized>(sim: &A, position: &A::Position, buffers: &mut [&mut [Option<A::Position>]]) {
    let range = buffers.len();
    let position = sim.decode(position);
    assert!(sim.encode(position).is_some());
    let AntPosition { x, y } = position;
    let downrange_x = if x <= range { x } else { range };
    let downrange_y = if y <= range { y } else { range };
    let uprange_y = if sim.height() - y <= range { sim.height() - 1 - y  } else { range };
    let uprange_x = if sim.width() - x <= range { sim.height() - 1 - x } else { range };
    for r in 1..=range {
        let buffer = &mut buffers[r - 1];
        assert_eq!(buffer.len(), 4 * (1 + 2  * r) - 4);
        let down_start_x = min(downrange_x, r);
        let up_end_x = min(uprange_x, r);
        let down_start_y = min(downrange_y, r - 1);
        let up_end_y = min(uprange_y, r - 1);
        if r <= uprange_y {
            let mut start_i = r - down_start_x;
            for x in (x - down_start_x)..=(x + up_end_x) {
                buffer[start_i] = sim.encode(AntPosition { x, y: y + r });
                start_i += 1;
            }
        }
        if r <= uprange_x {
            let mut start_i = 1 + 2 * r + (r - 1 - up_end_y);
            for y in ((y - down_start_y)..=(y + up_end_y)).rev() {
                buffer[start_i] = sim.encode(AntPosition { x: x + r, y });
                start_i += 1;
            }
        }
        if r <= downrange_y {
            let mut start_i =  2 * (1 + 2 * r) - 2 + (r - up_end_x);
            for x in ((x - down_start_x)..=(x + up_end_x)).rev() {
                buffer[start_i] = sim.encode(AntPosition { x, y: y - r });
                start_i += 1;
            }
        }
        if r <= downrange_x {
            let mut start_i = 3 * (1 + 2 * r) - 2 + (r - 1 - down_start_y);
            for y in (y - down_start_y)..=(y + up_end_y) {
                buffer[start_i] = sim.encode(AntPosition { x: x - r, y });
                start_i += 1;
            }
        }
    }
}
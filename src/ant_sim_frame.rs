use crate::{Ant};
pub use non_max::NonMaxU8;
use crate::ant_sim::AntState;

#[derive(Debug)]
pub struct Neighbors<A: AntSim + ?Sized> {
    pub up: Option<A::Position>,
    pub up_left: Option<A::Position>,
    pub up_right: Option<A::Position>,
    pub left: Option<A::Position>,
    pub right: Option<A::Position>,
    pub down: Option<A::Position>,
    pub down_left: Option<A::Position>,
    pub down_right: Option<A::Position>,
}

pub struct AntPosition {
    pub x: usize,
    pub y: usize,
}

mod non_max {
    #[repr(transparent)]
    #[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
    pub struct NonMaxU8(u8);

    impl NonMaxU8 {
        pub const fn new(val: u8) -> Self {
            match Self::try_new(val) {
                Ok(val) => val,
                Err(_) => panic!("val is u8::MAX!"),
            }
        }
        pub const fn try_new(val: u8) -> Result<Self, ()> {
            if val < u8::MAX {
                Ok(NonMaxU8(val))
            } else {
                Err(())
            }
        }
        pub const fn get(self) -> u8 {
            self.0
        }
        pub const fn dec_by(self, other: u8) -> Self {
            NonMaxU8(self.0.saturating_sub(other))
        }
    }
}

#[derive(Clone)]
pub enum AntSimCell {
    Path {
        pheromone_food: NonMaxU8,
        pheromone_home: NonMaxU8,
    },
    Blocker,
    Home,
    Food {
        amount: u8,
    },
}

pub trait AntSim {
    type Position: Eq + Clone + Into<u64>;
    type Cells<'a>: Iterator<Item=(AntSimCell, Self::Position)> where Self: 'a;

    fn neighbors(&self, position: &Self::Position) -> Result<Neighbors<Self>, ()>;
    fn check_compatible(&self, other: &Self) -> bool;
    fn decode(&self, position: &Self::Position) -> AntPosition;
    fn encode(&self, position: AntPosition) -> Self::Position;
    fn cell(&self, position: &Self::Position) -> Option<AntSimCell>;
    fn set_cell(&mut self, position: &Self::Position, cell: AntSimCell);
    fn cells<'a>(&'a self) -> Self::Cells<'a>;
}

#[derive(Clone)]
pub struct AntSimulator<A: AntSim> {
    pub sim: A,
    pub ants: Vec<Ant<A>>,
    pub seed: u64,
    pub decay_rate: u8,
    pub decay_step: u8,
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

static POINTS3: [(f64, f64); 8] = [
    (3.0, 0.0),
    (2.0121320343559643, 2.1213203435596424),
    (0.0, 3.0),
    (-2.1213203435596424, 2.121320343559643),
    (-3.0, 0.0),
    (-2.121320343559643, -2.1213203435596424),
    (0.0, -3.0),
    (2.121320343559642, -2.121320343559643),
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
        const HAUL_AMOUNT: u8 = 20;
        for ant in &mut update_into.ants {
            let state = ant.state_mut().clone();
            match (self.sim.cell(ant.position()).unwrap(), state) {
                (AntSimCell::Food { amount }, AntState::Foraging) => {
                    let (haul_amount, new_cell) = if amount > HAUL_AMOUNT {
                        (HAUL_AMOUNT, AntSimCell::Food { amount: amount - HAUL_AMOUNT })
                    } else {
                        (amount, AntSimCell::Path { pheromone_food: NonMaxU8::new(0), pheromone_home: NonMaxU8::new(0) })
                    };
                    *ant.state_mut() = AntState::Hauling { amount: haul_amount };
                    update_into.sim.set_cell(ant.position(), new_cell);
                }
                (AntSimCell::Home, AntState::Hauling { .. }) => {
                    *ant.state_mut() = AntState::Foraging;
                }
                _ => {
                    update_into.seed += 1;
                    ant.move_to_next(update_into.seed, &POINTS3, &self.sim);
                }
            }
        }
        let mut decay_step = self.decay_step;
        self.sim.cells()
            .map(|(cell, pos): (AntSimCell, A::Position)| {
                decay_step = (decay_step + 1) % self.decay_rate;
                match cell {
                    AntSimCell::Path { pheromone_food, pheromone_home } => {
                        let decay_by = if decay_step == 0 { 1 } else { 0 };
                        let cell = AntSimCell::Path {
                            pheromone_food: pheromone_food.dec_by(decay_by),
                            pheromone_home: pheromone_home.dec_by(decay_by),
                        };
                        (cell, pos)
                    }
                    other => (other, pos)
                }
            })
            .for_each(|(cell, pos)| {
                update_into.sim.set_cell(&pos, cell);
            });
        for ant in &self.ants {
            let cell = update_into.sim.cell(ant.position()).unwrap();
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
            update_into.sim.set_cell(ant.position(), new_cell);
        }
        update_into.decay_step = self.decay_step;
    }
}

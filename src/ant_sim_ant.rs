use crate::ant_sim_frame::{AntSim, Neighbors, AntSimCell};

#[derive(Debug)]
pub struct Ant<A: AntSim + ?Sized> {
    position: A::Position,
    come_from: A::Position,
    state: AntState,
    explore_weight: f64
}

#[derive(Copy, Clone, Debug)]
pub enum AntState {
    Foraging, Hauling { amount: u8 }
}

impl<A: AntSim + ?Sized> Clone for Ant<A> where A::Position: Clone {
    fn clone(&self) -> Self {
        Self {
            position: self.position.clone(),
            come_from: self.come_from.clone(),
            state: self.state.clone(),
            explore_weight: self.explore_weight
        }
    }
}

impl<A: AntSim + ?Sized> Ant<A> {
    pub fn new_default(position: A::Position, explore_weight: f64) -> Self {
        Self {
            come_from: position.clone(),
            state: AntState::Foraging,
            position,
            explore_weight
        }
    }
    pub fn position(&self) -> &A::Position {
        &self.position
    }

    pub fn state(&self) -> &AntState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut AntState {
        &mut self.state
    }

    pub fn move_to_next(&mut self, seed: u64, points: &[(f64, f64); 8], on: &A) {
        let neighbors = on.neighbors(&self.position).expect("could not make neighbors");
        let mut new_position = None;
        let mut new_score = f64::NEG_INFINITY;
        let come_from = map_come_from_to_points(&neighbors, &self.come_from, points).unwrap_or((0.0, 0.0));
        let p_weight = match self.state {
            AntState::Foraging => (1.0, -0.25),
            AntState::Hauling { .. } => (-0.25, 1.0)
        };
        macro_rules! max_pos {
            (($n: ident, $i: literal)) => {
                if let Some(pos) = neighbors.$n {
                    let cell = on.cell(&pos).unwrap();
                    let this_point = points[$i];
                    let score = self.score_position(&pos, cell, come_from, this_point, p_weight, seed);
                    match score {
                        Some(score) if score > new_score => {
                            new_position = Some(pos);
                            new_score = score;
                        }
                        _ => {}
                    }
                }
            };
            (($n: ident, $i: literal), $(($rn: ident, $ri: literal)),+) => {
                max_pos!(($n, $i));
                max_pos!($(($rn, $ri)),*);
            }
        }
        max_pos!((up, 0), (up_left, 1), (left, 2), (down_left, 3), (down, 4), (down_right, 5), (right, 6), (up_right, 7));
        self.come_from = std::mem::replace(&mut self.position, new_position.unwrap());
    }

    fn score_position(&self, pos: &A::Position, cell: AntSimCell, come_from: (f64, f64), this_point: (f64, f64), p_weight: (f64, f64), seed: u64, ) -> Option<f64> {
        let p_score = match cell {
            AntSimCell::Path { pheromone_food, pheromone_home } => {
                f64::from(pheromone_food.get()) * p_weight.0 + f64::from(pheromone_home.get()) * p_weight.1
            }
            AntSimCell::Blocker => {
                return None;
            }
            AntSimCell::Home => {
                if let AntState::Hauling { .. } = self.state { 510.0 } else { 0.0 }
            }
            AntSimCell::Food { amount } => {
                if let AntState::Foraging= self.state { f64::from(amount) } else { 0.0 }
            }
        };
        let explore_score = f64::from(simple_hash2(pos.clone().into(), seed));
        let come_from_distance = {
            let dist = (this_point.0 - come_from.0, this_point.1 - come_from.1);
            ((dist.0 * dist.0) + (dist.1 * dist.1)).sqrt()
        };
        let score = (1.0- self.explore_weight) * p_score + self.explore_weight * explore_score;
        let score = score * (come_from_distance + 1.0);
        Some(score)
    }
}

fn map_come_from_to_points<A: AntSim + ?Sized>(neighbors: &Neighbors<A>, find: &A::Position, points: &[(f64, f64); 8]) -> Option<(f64, f64)> {
    macro_rules! find_come_from {
            (($n: ident, $i: literal)) =>{
                if matches!(&neighbors.$n, Some(pos) if pos == find){
                    return Some(points[$i]);
                }
            };
            (($n: ident, $i: literal), $(($rn: ident, $ri: literal)),+) => {
                find_come_from!(($n, $i));
                find_come_from!($(($rn, $ri)),*);
            }
    }
    find_come_from!((up, 0), (up_left, 1), (left, 2), (down_left, 3), (down, 4), (down_right, 5), (right, 6), (up_right, 7));
    return None;
}

pub fn simple_hash(a: u64, mut b: u64) -> u64 {
    b ^= 0xF7eA_A097_91CE_5D9A;
    let mut r = a.wrapping_mul(b);
    r ^= r >> 32;
    r = r.wrapping_add((!r) >> 4);
    r = r.wrapping_mul(0xDEF8_9E5D_254A_A78C);
    r ^= r >> 24;
    r
}

pub fn simple_hash2(a: u64, mut b: u64) -> u8 {
    b ^= 0xF7eA_A097_91CE_5D9A;
    let mut r = a.wrapping_mul(b);
    r ^= r >> 32;
    r = r.wrapping_add((!r) >> 4);
    r = r.wrapping_mul(0xDEF8_9E5D_254A_A78C);
    r ^= r >> 32;
    r ^= r >> 16;
    r ^= r >> 8;
    r as u8
}



use std::hash::{Hash, Hasher};
use crate::ant_sim_frame::{AntSim, Neighbors, AntSimCell};
use crate::neighbors;

#[derive(Debug)]
pub struct Ant<A: AntSim + ?Sized> {
    position: A::Position,
    last_position: A::Position,
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
            last_position: self.last_position.clone(),
            state: self.state.clone(),
            explore_weight: self.explore_weight
        }
    }
}

impl<A: AntSim + ?Sized> Ant<A> {
    pub fn new_default(position: A::Position, explore_weight: f64) -> Self {
        Self {
            last_position: position.clone(),
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

    /// Sets the last_position to the current position;
    pub fn stand_still(&mut self) {
        self.last_position = self.position.clone();
    }

    pub fn move_to_next(&mut self, seed: u64, points: &[(f64, f64); 8], on: &A) {
        let neighbors = on.neighbors(&self.position).expect("could not make neighbors");
        let mut new_position = None;
        let mut new_score = f64::NEG_INFINITY;
        let last_pos = map_come_from_to_points(&neighbors, &self.last_position, points).unwrap_or((0.0, 0.0));
        let p_weight = match self.state {
            AntState::Foraging => (1.0, -0.25),
            AntState::Hauling { .. } => (-0.25, 1.0)
        };
        macro_rules! max_pos {
            (($n: ident, $i: literal)) => {
                if let Some(pos) = neighbors.$n {
                    let cell = on.cell(&pos).unwrap();
                    let this_point = points[$i];
                    let score = self.score_position(&pos, cell, last_pos, this_point, p_weight, seed);
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
        self.last_position = std::mem::replace(&mut self.position, new_position.unwrap());
    }
    
    pub fn move_to_next2<H: Hasher + Default>(&mut self, seed: u64, points: &[(f64, f64); 8], on: &A, buffers: &mut [&mut [Option<A::Position>]]) {
        fn dist_of(a: (f64, f64), b: (f64, f64)) -> f64 {
            let vec = (a.0 - b.0, a.1 - b.1);
            let vec_len = f64::sqrt(vec.0*vec.0 + vec.1*vec.1);
            return vec_len;
        }
        assert!(buffers.len() >= 1);
        assert_eq!(buffers[0].len(), 8);
        
        neighbors(on, &self.position, buffers);
        let mut new_position;
        let mut new_score;
        let last_pos = buffers[0].iter().zip(points.iter())
            .find(|(n, _pos)| (*n).as_ref() == Some(&self.last_position))
            .map(|(_, p)| *p)
            .unwrap_or((0.0, 0.0));
        let (p_food_weight, p_home_weight) = match self.state {
            AntState::Foraging => (1.0, -0.25),
            AntState::Hauling { .. } => (-0.25, 1.0)
        };
        {
            let mut score = 0.0;
            for r in 0..buffers.len() {
                let mut p_home = 0u32;
                let mut p_food = 0u32;
                let mut count = 0.0;
                let mut special_count = 0u32;
                let buffer = &buffers[r];
                let start = buffer.len() - r * 2;
                for r in 0..(1 + r * 4) {
                    let pos = &buffer[(start + r) % buffer.len()];
                    let pos = if let Some(pos) = pos.as_ref() {
                        pos
                    } else {
                        continue;
                    };
                    count += 1.0;
                    //todo avoid blocker trap
                    match on.cell(pos).unwrap() {
                        AntSimCell::Path { pheromone_food, pheromone_home } => {
                            p_home += pheromone_home.get() as u32;
                            p_food += pheromone_food.get() as u32;
                        }
                        AntSimCell::Blocker => continue,
                        AntSimCell::Home =>
                            special_count += if matches!(self.state, AntState::Hauling {..}) { 1000 } else { 0 },
                        AntSimCell::Food { amount } =>
                            special_count += if matches!(self.state, AntState::Foraging) { amount as u32 * 8 } else { 0 }
                    }
                    if count == 0.0 { break; }
                    let p_score =f64::from(p_home) * p_home_weight + f64::from(p_food) * p_food_weight;
                    let avg_score =  (p_score + f64::from(special_count)) / count;
                    score += avg_score / f64::from(buffers.len() as u32);
                }
            }
            let first = &buffers[0][0];
            if let Some(first) = first.as_ref() {
                let explore_score = f64::from(simple_hash2::<A, H>(&first, seed));
                score = score * (1.0 - self.explore_weight) + self.explore_weight * explore_score;
                let dist_from_last_pos = dist_of(points[0], last_pos);
                score *= dist_from_last_pos;
                new_position = Some(first);
                new_score = score;
            } else  {
                new_position = None;
                new_score = f64::NEG_INFINITY;
            }
        }
        let mut edges_off = 0;
        for n in 1..buffers[0].len() {
            let is_edge = (n % 2) == 0;
            let mut score = 0.0;
            let l_mult = if is_edge { 4 } else { 2 };
            for r in 0..buffers.len() {
                let mut p_home = 0u32;
                let mut p_food = 0u32;
                let mut count = 0.0;
                let mut special_count = 0u32;
                let buffer = &buffers[r];
                let start = n + r * edges_off;
                for p in start..(start + 1 + l_mult * r) {
                    let pos = &buffer[p];
                    let pos = if let Some(pos) = pos.as_ref() {
                        pos
                    } else {
                        continue;
                    };
                    count += 1.0;
                    //todo avoid blocker trap
                    match on.cell(pos).unwrap() {
                        AntSimCell::Path { pheromone_food, pheromone_home } => {
                            p_home += pheromone_home.get() as u32;
                            p_food += pheromone_food.get() as u32;
                        }
                        AntSimCell::Blocker => continue,
                        AntSimCell::Home =>
                            special_count += if matches!(self.state, AntState::Hauling {..}) { 1000 } else { 0 },
                        AntSimCell::Food { amount } =>
                            special_count += if matches!(self.state, AntState::Foraging) { amount as u32 * 8 } else { 0 }
                    }
                    if count == 0.0 { break; }
                    let p_score =f64::from(p_home) * p_home_weight + f64::from(p_food) * p_food_weight;
                    let avg_score =  (p_score + f64::from(special_count)) / count;
                    score += avg_score / f64::from(buffers.len() as u32);
                }
            }
            if is_edge { edges_off += 2 }
            let pos = &buffers[0][n];
            if let Some(pos) = pos.as_ref() {
                let explore_score = f64::from(simple_hash2::<A, H>(&pos, seed));
                score = score * (1.0 - self.explore_weight) + self.explore_weight * explore_score;
                let dist_from_last_pos = dist_of(points[n], last_pos);
                score *= dist_from_last_pos;
                if score >  new_score {
                    new_position = Some(pos);
                    new_score = score;
                }
            }

        }
        self.last_position = std::mem::replace(&mut self.position, new_position.unwrap().clone());
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
        let explore_score = f64::from(simple_hash2::<A, fasthash::mum::Hasher64>(&pos, seed));
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

pub fn simple_hash2<A:AntSim + ?Sized, H: Hasher + Default>(a: &A::Position, b: u64) -> u8 {
    let mut h = H::default();
    a.hash(&mut h);
    b.hash(&mut h);
    let mut r = h.finish();
    r ^= r >> 32;
    r ^= r >> 16;
    r ^= r >> 8;
    return r as u8;
    /*
    b ^= 0xF7eA_A097_91CE_5D9A;
    let mut r = a.wrapping_mul(b);
    r ^= r >> 32;
    r = r.wrapping_add((!r) >> 4);
    r = r.wrapping_mul(0xDEF8_9E5D_254A_A78C);
    r ^= r >> 32;
    r ^= r >> 16;
    r ^= r >> 8;
    r as u8*/
}



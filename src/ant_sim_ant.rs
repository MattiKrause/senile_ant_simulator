use std::hash::{Hash, Hasher};
use std::ops::Not;
use crate::ant_sim::neighbors;
use crate::ant_sim_frame::{AntSim, AntSimCell};

#[derive(Debug)]
pub struct Ant<A: AntSim + ?Sized> {
    pub position: A::Position,
    pub last_position: A::Position,
    pub state: AntState,
    pub explore_weight: f64,
}

#[derive(Copy, Clone, Debug)]
pub enum AntState {
    Foraging,
    Hauling { amount: u16 },
}

impl<A: AntSim + ?Sized> Clone for Ant<A> where A::Position: Clone {
    fn clone(&self) -> Self {
        Self {
            position: self.position.clone(),
            last_position: self.last_position.clone(),
            state: self.state,
            explore_weight: self.explore_weight,
        }
    }
}

impl<A: AntSim + ?Sized> Ant<A> {
    pub fn new_default(position: A::Position, explore_weight: f64) -> Self {
        Self::new(position.clone(), position, explore_weight, AntState::Foraging)
    }
    pub fn new(position: A::Position, last_position: A::Position, explore_weight: f64, state: AntState) -> Self {
        Self {
            position,
            last_position,
            state,
            explore_weight
        }
    }
    pub fn position(&self) -> &A::Position {
        &self.position
    }

    pub fn last_position(&self) -> &A::Position {
        &self.position
    }

    pub fn state(&self) -> &AntState {
        &self.state
    }

    pub fn exploration_weight(&self) -> f64 {
        self.explore_weight
    }

    pub fn state_mut(&mut self) -> &mut AntState {
        &mut self.state
    }

    /// Sets the last position to the current position;
    pub fn stand_still(&mut self) {
        self.last_position = self.position.clone();
    }

    /// Evaluates all neighbors and moves to a random position, weighted by desirability
    /// * `seed`: the randomness seed
    /// * `points` is used to calculate the distance between the last position and the position being inspected,
    /// the weight of the position is then scaled by that distance
    /// * `on` is the board state
    /// * `buffers` buffers the neighbors of the position, each buffer should have the size of `index * 8`. The amount of buffers indicates the visual range
    ///
    /// # Panics
    /// This function panics if `buffers` is empty, if the buffers have an invalid size
    pub fn move_to_next2<H: Hasher + Default>(&mut self, seed: u64, points: &[(f64, f64); 8], on: &A, buffers: &mut [&mut [Option<A::Position>]]) {
        assert!(buffers.is_empty().not());
        assert_eq!(buffers[0].len(), 8);

        neighbors(on, &self.position, buffers);
        let mut new_position = None;
        let mut new_score = f64::NEG_INFINITY;
        let last_pos = buffers[0].iter().zip(points.iter())
            .find(|(n, _pos)| (*n).as_ref() == Some(&self.last_position))
            .map_or((0.0, 0.0), |(_, p)| *p);
        let (p_food_weight, p_home_weight) = match self.state {
            AntState::Foraging => (1.0, 0.0),
            AntState::Hauling { .. } => (0.0, 1.0)
        };
        {
            let pos = buffers[0][0].as_ref();
            let score = self.score_position2::<H, _, _>(pos, points[0], last_pos, seed, p_home_weight, p_food_weight, buffers, |buffer, r| {
                let start = buffer.len() - r * 2;
                (0..(1 + r * 4))
                    .map(move |i| (i + start) % buffer.len())
                    .map(|idx| buffer[idx].as_ref().and_then(|pos| on.cell(pos).map(|cell| (pos, cell))))
            });
            if let Some(score) = score {
                new_position = buffers[0][0].as_ref();
                new_score = score;
            }
        }
        let mut edges_off = 0;
        for (n, d_pos) in buffers[0].iter().enumerate().skip(1) {
            let is_edge = (n % 2) == 0;
            let l_mult = if is_edge { 4 } else { 2 };
            let score = self.score_position2::<H, _, _>(d_pos.as_ref(), points[n], last_pos, seed, p_home_weight, p_food_weight, buffers, |buffer, r| {
                let start = n + r * edges_off;
                (start..(start + 1 + l_mult * r))
                    .map(|idx| buffer[idx].as_ref().and_then(|pos| on.cell(pos).map(|cell| (pos, cell))))
            });
            if let Some(score) = score {
                if score > new_score {
                    new_position = d_pos.as_ref();
                    new_score = score;
                }
            }
            if is_edge {
                edges_off += 2;
            }
        }
        self.last_position = std::mem::replace(&mut self.position, new_position.unwrap().clone());
    }

    fn score_position2<'p, H: Hasher + Default, PI: Iterator<Item=Option<(&'p A::Position, AntSimCell)>>, P: Fn(&'p [Option<A::Position>], usize) -> PI>(
        &self, pos: Option<&A::Position>, this_points: (f64, f64), last_pos: (f64, f64), seed: u64, p_home_weight: f64, p_food_weight: f64, buffers: &'p [&'p mut [Option<A::Position>]], positions_of: P,
    ) -> Option<f64> {
        fn dist_of(a: (f64, f64), b: (f64, f64)) -> f64 {
            let vec = (a.0 - b.0, a.1 - b.1);
            let vec_len = f64::sqrt(vec.0 * vec.0 + vec.1 * vec.1);
            return vec_len;
        }
        let mut score = 0.0;
        let score_pos = pos?;
        for r in 0..buffers.len() {
            let mut p_home = 0u32;
            let mut p_food = 0u32;
            let mut count = 0.0;
            let mut special_count = 0u32;
            let buffer = &*buffers[r];
            let positions = positions_of(buffer, r);
            for pos in positions {
                let (_, cell) = if let Some(pos) = pos {
                    pos
                } else {
                    continue;
                };
                count += 1.0;
                //todo avoid blocker trap
                match cell {
                    AntSimCell::Path { pheromone_food, pheromone_home } => {
                        p_home += u32::from(pheromone_home.get());
                        p_food += u32::from(pheromone_food.get());
                    }
                    AntSimCell::Blocker => continue,
                    AntSimCell::Home =>
                        special_count += if matches!(self.state, AntState::Hauling {..}) { u32::from(u16::MAX) * 8 } else { 0 },
                    AntSimCell::Food { amount } =>
                        special_count += if matches!(self.state, AntState::Foraging) { u32::from(amount) * 8 } else { 0 }
                }
            }
            if count == 0.0 { break; }
            let p_score = f64::from(p_home) * p_home_weight + f64::from(p_food) * p_food_weight;
            let avg_score = (p_score + f64::from(special_count)) / count;
            score += avg_score / f64::from(buffers.len() as u32);
        }

        let explore_score = f64::from(simple_hash2::<A, H>(score_pos, seed));
        score = score * (1.0 - self.explore_weight) + self.explore_weight * explore_score;
        let dist_from_last_pos = dist_of(this_points, last_pos);
        score *= dist_from_last_pos;
        Some(score)
    }
}

#[allow(clippy::cast_possible_truncation)]
pub fn simple_hash2<A: AntSim + ?Sized, H: Hasher + Default>(a: &A::Position, b: u64) -> u16 {
    let mut h = H::default();
    a.hash(&mut h);
    b.hash(&mut h);
    let mut r = h.finish();
    r ^= r >> 32;
    r ^= r >> 16;
    return r as u16;
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



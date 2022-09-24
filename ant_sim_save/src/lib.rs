pub mod save_subsystem;

use ant_sim::ant_sim::{AntSimConfig, AntSimulator, AntVisualRangeBuffer};
use ant_sim::ant_sim_ant::{Ant, AntState};
use ant_sim::ant_sim_frame::{AntPosition, AntSim, AntSimCell, NonMaxU16};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct AntSimData {
    env: AntSimEnv,
    ants: Vec<AntSimAntData>,
    board: AntSimBoardData
}

#[derive(Serialize, Deserialize)]
struct AntSimEnv {
    seed: u64,
    decay_rate: u16,
    haul_amount: u16,
    points: [(f64, f64); 8],
    ant_visual_range: u8,
    dimensions: Dimensions
}

#[derive(Serialize, Deserialize)]
struct AntSimAntData {
    position: u64,
    last_position: u64,
    exploration_factor: f64,
    state: AntSimAntStateData
}

#[derive(Serialize, Deserialize)]
enum AntSimAntStateData {
    Foraging, Hauling { amount: u16 }
}

#[derive(Serialize, Deserialize)]
struct AntSimBoardData {
    blockers: Vec<u64>,
    homes: Vec<u64>,
    foods: Vec<(u64, u16)>,
    paths_with_pheromones: Vec<(u64, AntSimPathPheromoneData)>
}

#[derive(Serialize, Deserialize)]
struct AntSimPathPheromoneData {
    p_h: u16,
    p_f: u16,
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
pub struct Dimensions {
    pub width: u64,
    pub height: u64
}

impl Dimensions {
    fn decode(&self, pos: u64) -> Result<AntPosition, ()> {
        let x = pos % self.width;
        let y = pos / self.width;
        if y >= self.height {
            return Err(());
        }
        let x: usize = x.try_into().map_err(|_|())?;
        let y: usize = y.try_into().map_err(|_|())?;
        let pos = AntPosition { x, y };
        Ok(pos)
    }
    fn encode(&self, ant_pos: AntPosition) -> Result<u64, ()> {
        let x: u64 = ant_pos.x.try_into().map_err(|_|())?;
        let y: u64 = ant_pos.y.try_into().map_err(|_|())?;
        if x >= self.width || y >= self.height { return Err(())};
        Ok(y * self.width + x)
    }
}

impl AntSimData {
    pub fn try_into_board<A: AntSim>(self, get_a: impl FnOnce(Dimensions) -> Result<A, ()>) -> Result<AntSimulator<A>, String> {
        let mut a = get_a(self.env.dimensions).map_err(|_| String::from("invalid dimensions"))?;
        let ants = self.ants.into_iter()
            .map(|ant| ant.try_into_ant(&a, &self.env.dimensions))
            .enumerate()
            .map(|(i, ant)| ant.map_err(|err| format!("failed to decode ant {i}: {err}")))
            .collect::<Result<Vec<_>, _>>()?;
        self.board.try_apply_to_board(&mut a, &self.env.dimensions)?;
        if self.env.ant_visual_range > 20 {
            return Err(String::from("ant visual range is to large"));
        }
        let config = AntSimConfig {
            distance_points: Box::new(self.env.points),
            food_haul_amount: self.env.haul_amount,
            pheromone_decay_amount: self.env.decay_rate,
            seed_step: ants.len() as u64,
            visual_range: AntVisualRangeBuffer::new(self.env.ant_visual_range as usize)
        };
        let sim = AntSimulator {
            sim: a,
            ants,
            seed: self.env.seed,
            config
        };
        Ok(sim)
    }
    pub fn from_state_sim<A: AntSim>(sim: &AntSimulator<A>) -> Result<Self, ()> {
        let env = AntSimEnv {
            seed: sim.seed,
            decay_rate: sim.config.pheromone_decay_amount,
            haul_amount: sim.config.food_haul_amount,
            points: *sim.config.distance_points,
            ant_visual_range: sim.config.visual_range.range().try_into().map_err(|_|())?,
            dimensions: Dimensions {
                width: sim.sim.width().try_into().map_err(|_|())?,
                height: sim.sim.height().try_into().map_err(|_|())?
            }
        };
        let ants = sim.ants.iter()
            .map(|it| AntSimAntData::try_from_ant(it, &sim.sim, &env.dimensions))
            .collect::<Result<Vec<_>, _>>()?;
        let board = AntSimBoardData::try_from_board(&sim.sim, &env.dimensions)?;
        let res = Self {
            env,
            ants,
            board
        };
        Ok(res)
    }
}

impl AntSimAntData {
    fn try_into_ant<A: AntSim + ?Sized>(self, on: &A, dimensions: &Dimensions) -> Result<Ant<A>, String> {
        let pos = dimensions
            .decode(self.position)
            .and_then(|pos| on.encode(pos).ok_or(()))
            .map_err(|_| String::from("invalid ant position"))?;
        let last_pos = dimensions.decode(self.last_position)
            .and_then(|pos| on.encode(pos).ok_or(()))
            .map_err(|_| String::from("invalid ant last position"))?;
        let state = match self.state {
            AntSimAntStateData::Foraging => AntState::Foraging,
            AntSimAntStateData::Hauling { amount } => AntState::Hauling { amount }
        };
        let ant = Ant::new(pos, last_pos, self.exploration_factor, state);
        Ok(ant)
    }
    fn try_from_ant<A: AntSim + ?Sized>(ant: &Ant<A>, on: &A, dimensions: &Dimensions) -> Result<AntSimAntData, ()> {
        let state = match ant.state() {
            AntState::Foraging => AntSimAntStateData::Foraging,
            AntState::Hauling { amount } => AntSimAntStateData::Hauling { amount: *amount }
        };
        let data= Self {
            position: dimensions.encode(on.decode(ant.position()))?,
            last_position: dimensions.encode(on.decode(ant.last_position()))?,
            exploration_factor: ant.exploration_weight(),
            state
        };
        Ok(data)
    }
}

impl AntSimBoardData {
    fn try_apply_to_board<A: AntSim + ?Sized> (self, board: &mut A, dimensions: &Dimensions) -> Result<(), String> {
        //macro to have access to local variables
        macro_rules! decode_pos {
            ($pos: expr, $err: expr) => {
                dimensions.decode($pos)
                .and_then(|pos| board.encode(pos).ok_or(()))
                .map_err(|_| $err)?
            };
        }
        for (i, pos) in self.blockers.into_iter().enumerate()  {
            let pos = decode_pos!(pos, format!("failed to decode blocker position {i}"));
            board.set_cell(&pos, AntSimCell::Blocker)
        }
        for (i, pos) in self.homes.into_iter().enumerate() {
            let pos = decode_pos!(pos, format!("failed to decode home position {i}"));
            board.set_cell(&pos, AntSimCell::Home)
        }
        for  (i, (pos, amount)) in self.foods.into_iter().enumerate() {
            let pos = decode_pos!(pos, format!("failed to decode food position for food {i}"));
            board.set_cell(&pos, AntSimCell::Food { amount });
        }
        for (i, (pos, p_data)) in self.paths_with_pheromones.into_iter().enumerate() {
            let pos = decode_pos!(pos, format!("failed to decode path {i}"));
            let cell = p_data.to_cell().map_err(|err| format!("failed to decode path {i}: {err}"))?;
            board.set_cell(&pos, cell);
        }
        Ok(())
    }
    fn try_from_board<A: AntSim>(board: &A, dimensions: &Dimensions) -> Result<Self, ()> {
        let mut result = Self {
            blockers: Vec::new(),
            homes: Vec::with_capacity(1),
            foods: Vec::new(),
            paths_with_pheromones: Vec::new(),
        };
        board.cells()
            .map(|(cell, pos)| (cell, board.decode(&pos)))
            .map(|(cell, pos)| dimensions.encode(pos).with(cell))
            .try_for_each(|cell| {
                Result::<(u64, AntSimCell), ()>::map(cell, |(pos, cell)| match cell {
                    AntSimCell::Path { pheromone_food, pheromone_home } => {
                        let pheromone_food = pheromone_food.get();
                        let pheromone_home = pheromone_home.get();
                        if pheromone_food != 0 || pheromone_home != 0 {
                            result.paths_with_pheromones.push((pos, AntSimPathPheromoneData { p_h: pheromone_home, p_f: pheromone_food }));
                        }
                    }
                    AntSimCell::Blocker => result.blockers.push(pos),
                    AntSimCell::Home => result.homes.push(pos),
                    AntSimCell::Food { amount } => result.foods.push((pos, amount))
                })
            })?;
        Ok(result)
    }
}

impl AntSimPathPheromoneData {
    fn to_cell(self) -> Result<AntSimCell, String> {
        let p_food = NonMaxU16::try_new(self.p_f).map_err(|_| String::from("invalid food pheromone"))?;
        let p_home = NonMaxU16::try_new(self.p_h).map_err(|_| String::from("invalid home pheromone"))?;
        Ok(AntSimCell::Path { pheromone_food: p_food, pheromone_home: p_home })
    }
}

trait WithExtTrait<T> {
    type Out;
    fn with(self, with: T) -> Self::Out;
}

impl <U, E, T> WithExtTrait<T> for Result<U, E>{
    type Out = Result<(U, T), E>;

    fn with(self, with: T) -> Self::Out {
        self.map(|u| (u, with))
    }
}
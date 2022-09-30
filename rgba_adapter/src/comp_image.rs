use ant_sim::ant_sim::AntSimulator;
use ant_sim::ant_sim_ant::AntState;
use ant_sim::ant_sim_frame::{AntPosition, AntSim, AntSimCell};
use crate::SetRgb;

pub fn draw_to_buf<A: AntSim>(sim: &AntSimulator<A>, mut frame: impl SetRgb) {
    fn set_pixel(width: usize, pos: AntPosition, val: [u8; 3], into: &mut impl SetRgb) {
        into.set_rgb(pos.y * width + pos.x, val);
    }
    assert_eq!(sim.sim.width() * sim.sim.height(), frame.len());
    for cell in sim.sim.cells() {
        let (cell, pos): (AntSimCell, A::Position) = cell;
        let pos = sim.sim.decode(&pos);
        let color = match cell {
            AntSimCell::Path { pheromone_food, pheromone_home } => {
                [(pheromone_food.get() / 256u16) as u8, 0, (pheromone_home.get() / 256u16) as u8]
            }
            AntSimCell::Blocker => {
                [0xAF, 0xAF, 0xAF]
            }
            AntSimCell::Home => {
                [0xFF, 0xFF, 0x00]
            }
            AntSimCell::Food { amount } => {
                [0, (amount / 256u16) as u8, 0]
            }
        };
        set_pixel(sim.sim.width(), pos, color, &mut frame);
    }
    for ant in &sim.ants {
        let pos = sim.sim.decode(ant.position());
        let color = match ant.state(){
            AntState::Foraging => [0xFF, 0xFF, 0xFF],
            AntState::Hauling { amount }=> {
                let amount  = (*amount / 256u16) as u8 * (u8::MAX / 2);
                [0xFF - amount, 0xFF, 0xFF - amount]
            }
        };
        set_pixel(sim.sim.width(), pos, color, &mut frame);
    }
}
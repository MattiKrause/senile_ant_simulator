use crate::ant_sim_frame::{AntPosition, AntSim, AntSimCell, Neighbors, NonMaxU8};

#[derive(Clone)]
pub struct AntSimVecImpl {
    contains: Vec<AntSimCellImpl>,
    height: usize,
    width: usize,
}
#[derive(Eq, PartialEq, Copy, Clone)]
pub struct AntPositionImpl(usize);

#[derive(Clone)]
struct AntSimCellImpl  {
    p1: u8, p2: u8
}

impl AntSimCellImpl {
    pub fn to_cell(&self) -> AntSimCell {
        if self.p2 == u8::MAX {
            AntSimCell::Food {
                amount: self.p1
            }
        } else if self.p1 == u8::MAX {
            debug_assert!(self.p2 < 2);
            if self.p1 == 0 {
                AntSimCell::Blocker
            } else {
                AntSimCell::Home
            }
        } else {
            AntSimCell::Path {
                pheromone_food: NonMaxU8::new(self.p1),
                pheromone_home: NonMaxU8::new(self.p2)
            }
        }
    }
    pub const fn from_cell(cell: AntSimCell) -> AntSimCellImpl {
        match cell {
            AntSimCell::Path { pheromone_food, pheromone_home } => {
                Self {
                    p1: pheromone_food.get(),
                    p2: pheromone_home.get()
                }
            }
            AntSimCell::Blocker => Self {
                p1: u8::MAX,
                p2: 0
            },
            AntSimCell::Home => Self {
                p1: u8::MAX,
                p2: 1
            },
            AntSimCell::Food { amount } => {
                Self {
                    p1: amount,
                    p2: u8::MAX
                }
            }
        }
    }
}

impl Into<u64> for AntPositionImpl {
    fn into(self) -> u64 {
        self.0 as u64
    }
}

impl AntSimVecImpl {
    pub fn new(width: usize, height: usize) -> Result<Self, ()> {
        if width == 0 || height == 0 || width.overflowing_mul(height).1 {
            return Err(());
        }
        let mut contains = Vec::with_capacity(width * height);
        for _ in 0..(height * width) {
            contains.push(AntSimCellImpl::from_cell(AntSimCell::Path { pheromone_food: NonMaxU8::new(0), pheromone_home: NonMaxU8::new(0) }));
        }
        Ok(Self {
            contains,
            height,
            width
        })
    }
}

impl AntSim for AntSimVecImpl {
    type Position = AntPositionImpl;
    type Cells<'a> = CellIterImpl<'a> where Self: 'a;

    fn neighbors(&self, position: &Self::Position) -> Result<Neighbors<Self>, ()> {
        if position.0 > self.contains.len() {
            return Err(());
        }
        let AntPosition { y, x } = self.decode(position);
        macro_rules! check_pos {
            ($y: expr, $x: expr) => {
                {
                    if $y < self.height && $x < self.width {
                        Some(self.encode(AntPosition { y: $y, x: $x }))
                    } else {
                        None
                    }
                }
            };
        }
        let neighbors = Neighbors {
            up: check_pos!(y + 1, x),
            up_left: check_pos!(y + 1, x.wrapping_sub(1)),
            up_right: check_pos!(y + 1, x + 1),
            left: check_pos!(y, x.wrapping_sub(1)),
            right: check_pos!(y, x + 1),
            down: check_pos!(y.wrapping_sub(1), x),
            down_left: check_pos!(y.wrapping_sub(1), x.wrapping_sub(1)),
            down_right: check_pos!(y.wrapping_sub(1), x + 1)
        };
        Ok(neighbors)
    }

    fn check_compatible(&self, other: &Self) -> bool {
        self.contains.len() == other.contains.len() && self.height == other.height && self.width == other.width
    }

    fn decode(&self, position: &AntPositionImpl) -> AntPosition {
        AntPosition {
            y: position.0 / self.width,
            x: position.0 % self.width
        }
    }
    fn encode(&self, position: AntPosition) -> AntPositionImpl {
        let AntPosition { x, y } = position;
        AntPositionImpl(y * self.width + x)
    }

    fn cell(&self, position: &Self::Position) -> Option<AntSimCell> {
        self.contains.get(position.0).map(AntSimCellImpl::to_cell)
    }

    fn set_cell(&mut self, position: &Self::Position, set_cell: AntSimCell) {
        if let Some(cell) = self.contains.get_mut(position.0) {
            *cell = AntSimCellImpl::from_cell(set_cell);
        }
    }

    fn cells<'a>(&'a self) -> Self::Cells<'a> {
        CellIterImpl {
            sim: self,
            index: AntPositionImpl(0)
        }
    }
}

pub struct CellIterImpl<'a> {
    sim: &'a AntSimVecImpl,
    index: AntPositionImpl
}

impl <'a> Iterator for CellIterImpl<'a> {
    type Item = (AntSimCell, AntPositionImpl);

    fn next(&mut self) -> Option<Self::Item> {
        let cell = self.sim.contains.get(self.index.0)?;
        let cell = cell.to_cell();
        let res = Some((cell, self.index.clone()));
        self.index.0 += 1;
        return res;
    }
}
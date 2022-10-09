use crate::ant_sim_frame::{AntPosition, AntSim, AntSimCell, NonMaxU16};

#[derive(Clone)]
pub struct AntSimVecImpl {
    contains: Vec<AntSimCellImpl>,
    height: usize,
    width: usize,
}
#[derive(Eq, PartialEq, Copy, Clone, Hash)]
#[repr(transparent)]
pub struct AntPositionImpl(usize);

#[derive(Clone)]
pub struct AntSimCellImpl  {
    p1: u16, p2: u16
}

impl AntSimCellImpl {
    #[inline]
    #[must_use]
    pub fn to_cell(&self) -> AntSimCell {
        if self.p2 == u16::MAX {
            AntSimCell::Food {
                amount: self.p1
            }
        } else if self.p1 == u16::MAX {
            debug_assert!(self.p2 < 2);
            if self.p2 == 0 {
                AntSimCell::Blocker
            } else {
                AntSimCell::Home
            }
        } else {
            AntSimCell::Path {
                pheromone_food: NonMaxU16::new(self.p1),
                pheromone_home: NonMaxU16::new(self.p2)
            }
        }
    }
    #[inline]
    #[must_use]
    pub const fn from_cell(cell: AntSimCell) -> AntSimCellImpl {
        match cell {
            AntSimCell::Path { pheromone_food, pheromone_home } => {
                Self {
                    p1: pheromone_food.get(),
                    p2: pheromone_home.get()
                }
            }
            AntSimCell::Blocker => Self {
                p1: u16::MAX,
                p2: 0
            },
            AntSimCell::Home => Self {
                p1: u16::MAX,
                p2: 1
            },
            AntSimCell::Food { amount } => {
                Self {
                    p1: amount,
                    p2: u16::MAX
                }
            }
        }
    }
}
#[derive(Debug)]
pub enum NewAntSimVecImplError {
    DimensionZero, DimensionTooLarge, OutOfMemory
}

impl AntSimVecImpl {
    /// Creates a new [AntSimVecImpl] with the specified dimensions
    /// # Errors
    /// Returns an error if either the height or the width is zero, if the dimensions exceed [isize::MAX] or if the allocator failed
    #[inline]
    pub fn new(width: usize, height: usize) -> Result<Self, NewAntSimVecImplError> {
        if width == 0 || height == 0 {
            return Err(NewAntSimVecImplError::DimensionZero)
        }
        if width.overflowing_mul(height).1 || isize::try_from(width * height).is_err() {
            return Err(NewAntSimVecImplError::DimensionTooLarge)
        }
        let size = width * height;
        let mut contains = Vec::new();
        contains.try_reserve_exact(size).map_err(|_| NewAntSimVecImplError::OutOfMemory)?;
        for _ in 0..size {
            contains.push(AntSimCellImpl::from_cell(AntSimCell::Path { pheromone_food: NonMaxU16::new(0), pheromone_home: NonMaxU16::new(0) }));
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
    //type Cells<'a> = CellIterImpl<'a> where Self: 'a;
    type Cells<'a> = core::iter::Map<core::iter::Enumerate<core::slice::Iter<'a, AntSimCellImpl>>, fn((usize, &'a AntSimCellImpl)) -> (AntSimCell, Self::Position)> where Self: 'a;

    fn check_compatible(&self, other: &Self) -> bool {
        self.contains.len() == other.contains.len() && self.height == other.height && self.width == other.width
    }

    #[inline]
    fn decode(&self, position: &AntPositionImpl) -> AntPosition {
        AntPosition {
            y: position.0 / self.width,
            x: position.0 % self.width
        }
    }
    #[inline]
    #[must_use]
    fn encode(&self, position: AntPosition) -> Option<AntPositionImpl> {
        let AntPosition { x, y } = position;
        if x < self.width && y < self.height {
            Some(AntPositionImpl(y * self.width + x))
        } else {
            None
        }

    }

    #[inline]
    #[must_use]
    fn cell(&self, position: &Self::Position) -> Option<AntSimCell> {
        self.contains.get(position.0).map(AntSimCellImpl::to_cell)
    }

    #[inline]
    fn set_cell(&mut self, position: &Self::Position, set_cell: AntSimCell) {
        if let Some(cell) = self.contains.get_mut(position.0) {
            *cell = AntSimCellImpl::from_cell(set_cell);
        }
    }

    #[inline]
    fn cells(&self) -> Self::Cells<'_> {
        self.contains.iter().enumerate().map(|(i, c)| (c.to_cell(), AntPositionImpl(i)))
    }

    #[inline]
    fn width(&self) -> usize {
        self.width
    }

    #[inline]
    fn height(&self) -> usize {
        self.height
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
        let res = Some((cell, self.index));
        self.index.0 += 1;
        return res;
    }
}
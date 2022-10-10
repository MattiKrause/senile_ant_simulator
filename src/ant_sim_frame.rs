use std::cmp::min;
use std::hash::Hash;
pub use non_max::*;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct AntPosition {
    pub x: usize,
    pub y: usize,
}

impl AntPosition {
    #[inline]
    #[must_use]
    pub fn clamp_to(self, pos: AntPosition) -> AntPosition {
        Self {
            x: min(self.x, pos.x),
            y: min(self.y, pos.y)
        }
    }
}

mod non_max {
    #[repr(transparent)]
    #[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
    pub struct NonMaxU16(u16);

    impl NonMaxU16 {
        /// Constructs a new [NonMaxU16] value from the given value
        /// # Panics
        /// Panics if the value is equals to `u16::MAX`
        #[inline]
        #[must_use]
        pub const fn new(val: u16) -> Self {
            match Self::try_new(val) {
                Ok(val) => val,
                Err(()) => panic!("val is u16::MAX!"),
            }
        }
        /// Tries to construct a [NonMaxU16] value from the given value
        /// # Errors
        /// Returns an error if the value is equals to `u16::MAX`
        #[inline]
        #[allow(clippy::result_unit_err)]
        pub const fn try_new(val: u16) -> Result<Self, ()> {
            if val < u16::MAX {
                Ok(NonMaxU16(val))
            } else {
                Err(())
            }
        }
        #[inline]
        #[must_use]
        pub const fn get(self) -> u16 {
            self.0
        }
        #[inline]
        #[must_use]
        pub const fn dec_by(self, other: u16) -> Self {
            NonMaxU16(self.0.saturating_sub(other))
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AntSimCell {
    Path {
        pheromone_food: NonMaxU16,
        pheromone_home: NonMaxU16,
    },
    Blocker,
    Home,
    Food {
        amount: u16,
    },
}

pub trait AntSim {
    type Position: Eq + Clone + Hash;
    type Cells<'a>: Iterator<Item=(AntSimCell, Self::Position)> where Self: 'a;

    fn check_compatible(&self, other: &Self) -> bool;
    #[must_use]
    fn decode(&self, position: &Self::Position) -> AntPosition;
    #[must_use]
    fn encode(&self, position: AntPosition) -> Option<Self::Position>;
    #[must_use]
    fn cell(&self, position: &Self::Position) -> Option<AntSimCell>;
    fn set_cell(&mut self, position: &Self::Position, cell: AntSimCell);
    #[must_use]
    fn cells(&self) -> Self::Cells<'_>;
    #[must_use]
    fn width(&self) -> usize;
    #[must_use]
    fn height(&self) -> usize;
    #[must_use]
    fn cell_count(&self) -> usize { self.width() * self.height() }
}

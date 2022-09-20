use std::hash::Hash;
pub use non_max::*;

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

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct AntPosition {
    pub x: usize,
    pub y: usize,
}

mod non_max {

    #[repr(transparent)]
    #[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
    pub struct NonMaxU16(u16);

    impl NonMaxU16 {
        pub const fn new(val: u16) -> Self {
            match Self::try_new(val) {
                Ok(val) => val,
                Err(_) => panic!("val is u16::MAX!"),
            }
        }
        pub const fn try_new(val: u16) -> Result<Self, ()> {
            if val < u16::MAX {
                Ok(NonMaxU16(val))
            } else {
                Err(())
            }
        }
        pub const fn get(self) -> u16 {
            self.0
        }
        pub const fn dec_by(self, other: u16) -> Self {
            NonMaxU16(self.0.saturating_sub(other))
        }
    }
}

#[derive(Clone)]
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

    fn neighbors(&self, position: &Self::Position) -> Result<Neighbors<Self>, ()>;
    fn check_compatible(&self, other: &Self) -> bool;
    fn decode(&self, position: &Self::Position) -> AntPosition;
    fn encode(&self, position: AntPosition) -> Option<Self::Position>;
    fn cell(&self, position: &Self::Position) -> Option<AntSimCell>;
    fn set_cell(&mut self, position: &Self::Position, cell: AntSimCell);
    fn cells<'a>(&'a self) -> Self::Cells<'a>;
    fn width(&self) -> usize;
    fn height(&self) -> usize;
}

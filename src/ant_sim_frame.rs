pub use non_max::NonMaxU8;

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

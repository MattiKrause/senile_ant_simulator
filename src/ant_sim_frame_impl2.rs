use crate::ant_sim_frame::{AntPosition, AntSim, AntSimCell, NonMaxU16};
use crate::ant_sim_frame_impl::AntSimCellImpl;

const FOLD_SIZE: usize = FOLD_HEIGHT * FOLD_WIDTH;
const FOLD_WIDTH: usize = 8;
const FOLD_HEIGHT: usize = 8;

#[repr(transparent)]
struct AntSimCellFold([AntSimCellImpl; FOLD_SIZE]);

#[derive(Debug)]
pub enum NewAntSimFoldImplError {
    DimensionZero, DimensionTooLarge, OutOfMemory
}

#[repr(transparent)]
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct AntPositionImplFold(usize);

#[derive(Clone)]
pub struct AntSimFoldImpl {
    width: usize,
    height: usize,
    content: Box<[[AntSimCellImpl; FOLD_SIZE]]>
}

impl AntSimFoldImpl {
    pub fn new(width: usize, height: usize) -> Result<Self, NewAntSimFoldImplError> {
        if (width.overflowing_mul(height)).1 {
            return Err(NewAntSimFoldImplError::DimensionTooLarge);
        }
        //let cell_count = width * height;
        let fold_count = Self::fold_count(width, height);
        const FILL_CELL: AntSimCellImpl = AntSimCellImpl::from_cell(AntSimCell::Path { pheromone_food: NonMaxU16::new(0), pheromone_home: NonMaxU16::new(0) });
        let mut content = Vec::new();
        content.try_reserve_exact(fold_count).map_err(|_| NewAntSimFoldImplError::OutOfMemory)?;
        for _ in 0..fold_count {
            content.push([FILL_CELL; FOLD_SIZE])
        }
        let inst = Self {
            width,
            height,
            content: content.into_boxed_slice()
        };
        Ok(inst)
    }
    fn fold_count(width: usize, height: usize) -> usize {
        let fold_width = div_round_up(width, FOLD_WIDTH);
        let fold_height = div_round_up(height, FOLD_HEIGHT);
        fold_width * fold_height
    }
}

impl AntSim for AntSimFoldImpl {
    type Position = AntPositionImplFold;
    type Cells<'a> = core::iter::Map<core::iter::Enumerate<core::iter::Flatten< core::slice::Iter<'a, [AntSimCellImpl; FOLD_SIZE]>>>, fn((usize, &'a AntSimCellImpl)) -> (AntSimCell, Self::Position)> where Self: 'a;

    #[inline]
    fn check_invariant(&self) {
        assert_eq!(Self::fold_count(self.width(), self.height()), self.content.len());
        assert!(!self.content.is_empty());
        assert!(!self.width.overflowing_mul(self.height).1);
    }

    #[inline]
    fn check_compatible(&self, other: &Self) -> bool {
        self.width == other.width && self.height == other.height
    }

    #[inline]
    fn decode(&self, position: &Self::Position) -> AntPosition {
        let fold_num = position.0 / FOLD_SIZE;
        let fold_off = position.0 % FOLD_SIZE;
        let x = (fold_num % div_round_up(self.width, FOLD_WIDTH)) * FOLD_WIDTH + fold_off % FOLD_WIDTH;
        let y = (fold_num / div_round_up(self.width, FOLD_WIDTH)) * FOLD_HEIGHT + fold_off / FOLD_WIDTH;
        AntPosition { x, y }
    }

    #[inline]
    fn encode(&self, position: AntPosition) -> Option<Self::Position> {
        if position.x < self.width && position.y < self.height {
            let fold_num = (position.y / FOLD_HEIGHT) * div_round_up(self.width, FOLD_WIDTH) + (position.x / FOLD_WIDTH);
            let fold_off = (position.y % FOLD_HEIGHT) * FOLD_WIDTH + position.x % FOLD_WIDTH;
            // position.y % FOLD_HEIGHT * FOLD_WIDTH + position.x % FOLD_WIDTH <= (FOLD_HEIGHT - 1) * FOLD_WIDTH + FOLD_WIDTH - 1 = FOLD_HEIGHT * FOLD_WIDTH - 1 = FOLD_SIZE - 1
            let repr = fold_num * FOLD_SIZE + fold_off;
            let pos = AntPositionImplFold(repr);
            if !self.width.overflowing_mul(self.height).1 && Self::fold_count(self.width(), self.height()) == self.content.len() {
                // position.x < self.width => position.x <= self.width - 1 => position.x / FOLD_WIDTH <= (self.width - 1) / FOLD_WIDTH
                // (self.width - 1) / FOLD_WIDTH < div_round_up(self.width - 1):
                //      self.width % FOLD_WIDTH == 0 => (self.width - 1) % FOLD_WIDTH != 0 => (self.width - 1) / FOLD_WIDTH < self.width / FOLD_WIDTH
                //      self.width & FOLD_WIDTH != 0 => (self.width - 1) / FOLD_WIDTH < div_round_up(self.width, FOLD_WIDTH) => self.width / FOLD_WIDTH < self.width / FOLD_WIDTH + 1
                // same procedure for self.height/FOLD_HEIGHT
                //    position.x / FOLD_WIDTH <= (self.width -  1) / FOLD_WIDTH < div_round_up(self.width, FOLD_WIDTH)
                //    position.y / FOLD_HEIGHT < (self.height - 1) / FOLD_HEIGHT < div_round_up(self.height, FOLD_HEIGHT)
                // => position.x / FOLD_WIDTH <= position.x < self.width && position.y / FOLD_HEIGHT <= position.y < self.height
                // => (position.x / FOLD_WIDTH) * (position.y / FOLD_HEIGHT) will not overflow
                // therefor (position.x / FOLD_WIDTH) * (position.y / FOLD_HEIGHT) < Self::fold_count(self.width, self.height)
                //          => self.content.get((position.x / FOLD_WIDTH) * (position.y / FOLD_HEIGHT)).is_some()
                if self.cell(&pos).is_none() {
                    unsafe {
                        std::hint::unreachable_unchecked()
                    }
                }
            }
            Some(pos)
        } else {
            None
        }
    }

    #[inline]
    fn cell(&self, position: &Self::Position) -> Option<AntSimCell> {
        self.content
            .get(position.0 / FOLD_SIZE)
            .map(|cell| &cell[position.0 % FOLD_SIZE])
            .map(AntSimCellImpl::to_cell)
    }

    #[inline]
    fn set_cell(&mut self, position: &Self::Position, cell: AntSimCell) {
        let cell = AntSimCellImpl::from_cell(cell);
        self.content[position.0 / 64][position.0 % 64] = cell;
    }

    #[inline]
    fn cells(&self) -> Self::Cells<'_> {
        #[inline]
        fn map_to_inner(m: &AntSimCellFold) -> &[AntSimCellImpl; FOLD_SIZE] { &m.0 }
        fn map_to_final((i, c): (usize, &AntSimCellImpl)) -> (AntSimCell, AntPositionImplFold) {
            (c.to_cell(), AntPositionImplFold(i))
        }
        self.content
            .iter()
            .flatten()
            .enumerate()
            .map(map_to_final)
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

fn div_round_up(div: usize, by: usize) -> usize {
    div / by + if div % by != 0 { 1 } else { 0 }
}



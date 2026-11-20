#[path = "io_counting.rs"]
mod counting;
#[path = "io_join.rs"]
mod join;

pub use counting::{ByteCounter, CountingStream};
pub use join::{join, join_counted};

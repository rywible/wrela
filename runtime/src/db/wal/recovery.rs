use crate::db::wal::format::Record;
use crate::db::wal::segment::WalSegment;
use std::io;

pub fn recover(segment: &WalSegment) -> io::Result<Vec<Record>> {
    segment.replay()
}

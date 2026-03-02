use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeformationTileV1 {
    pub tile_id: u64,
    pub height_delta_milli: i64,
    pub wetness_milli: i64,
    pub compaction_milli: i64,
}

pub fn relax_tiles(tiles: &mut [DeformationTileV1], relax_milli: i64) {
    for tile in tiles {
        tile.height_delta_milli = approach_zero(tile.height_delta_milli, relax_milli);
        tile.wetness_milli = approach_zero(tile.wetness_milli, relax_milli);
        tile.compaction_milli = approach_zero(tile.compaction_milli, relax_milli / 2);
    }
}

fn approach_zero(value: i64, step: i64) -> i64 {
    if value > 0 {
        (value - step).max(0)
    } else if value < 0 {
        (value + step).min(0)
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::{DeformationTileV1, relax_tiles};

    #[test]
    fn deformation_relaxation_converges_toward_zero() {
        let mut tiles = vec![DeformationTileV1 {
            tile_id: 1,
            height_delta_milli: 100,
            wetness_milli: 80,
            compaction_milli: 60,
        }];
        relax_tiles(&mut tiles, 25);
        assert_eq!(tiles[0].height_delta_milli, 75);
        assert_eq!(tiles[0].wetness_milli, 55);
        assert_eq!(tiles[0].compaction_milli, 48);
    }
}

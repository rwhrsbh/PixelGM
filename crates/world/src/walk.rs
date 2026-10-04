//! Moving the party about a location on foot.

use crate::model::{Exit, Player, TILE, World};
#[cfg(test)]
use crate::model::{MAP_H, MAP_W};

/// Half the width and half the height of the patch of ground the party's feet take up. It is
/// smaller than a cell, so a one-cell gap can be walked through without lining up exactly.
pub const FEET: (f32, f32) = (4.0, 3.0);

impl World {
    fn feet_fit(&self, x: f32, y: f32) -> bool {
        let location = self.here();
        let corners = [(x - FEET.0, y - FEET.1), (x + FEET.0, y - FEET.1), (x - FEET.0, y + FEET.1), (x + FEET.0, y + FEET.1)];
        corners.iter().all(|(cx, cy)| !location.blocked((cx / TILE as f32).floor() as i32, (cy / TILE as f32).floor() as i32, &self.tiles))
    }

    /// Moves the party by `dx`, `dy` pixels (y grows downwards). Each axis is tried on its
    /// own, so walking diagonally into a wall slides along it instead of stopping dead.
    /// Returns whether the party moved at all.
    pub fn walk(&mut self, dx: f32, dy: f32) -> bool {
        let Player { x, y } = self.player;
        let (w, h) = self.here().size();
        let limit = (w as f32 * TILE as f32, h as f32 * TILE as f32);
        // A pixel at a time, so the party ends flush against what stops it rather than a
        // whole stride short.
        let mut left = dx;
        while left != 0.0 {
            let stride = left.clamp(-1.0, 1.0);
            let nx = (self.player.x + stride).clamp(FEET.0, limit.0 - FEET.0);
            if nx == self.player.x || !self.feet_fit(nx, self.player.y) {
                break;
            }
            self.player.x = nx;
            left -= stride;
        }
        let mut left = dy;
        while left != 0.0 {
            let stride = left.clamp(-1.0, 1.0);
            let ny = (self.player.y + stride).clamp(FEET.1, limit.1 - FEET.1);
            if ny == self.player.y || !self.feet_fit(self.player.x, ny) {
                break;
            }
            self.player.y = ny;
            left -= stride;
        }
        self.player.x != x || self.player.y != y
    }

    /// The exit the party is standing on, if any.
    pub fn exit_here(&self) -> Option<&Exit> {
        let (x, y) = self.player.cell();
        self.here().exit_at(x.max(0) as usize, y.max(0) as usize)
    }

    /// The closest exit within `reach` pixels of the party, for telling the player where a
    /// way out leads before they step on it.
    pub fn exit_near(&self, reach: f32) -> Option<&Exit> {
        let distance = |e: &&Exit| {
            let centre = Player::in_cell(e.x, e.y);
            (centre.x - self.player.x).hypot(centre.y - self.player.y)
        };
        self.here().exits.iter().filter(|e| distance(e) <= reach).min_by(|a, b| distance(a).total_cmp(&distance(b)))
    }

    /// Steps off an exit onto the nearest free cell: used when the place beyond could not
    /// be drawn and the party stays where it is.
    pub fn step_back(&mut self) {
        let (x, y) = self.player.cell();
        let location = self.here();
        let free = |(cx, cy): (usize, usize)| !location.blocked(cx as i32, cy as i32, &self.tiles) && location.exit_at(cx, cy).is_none();
        let (w, h) = self.here().size();
        let start = (x.clamp(0, w as i32 - 1) as usize, y.clamp(0, h as i32 - 1) as usize);
        if let Some((cx, cy)) = crate::path::nearest(start, free) {
            self.player = Player::in_cell(cx, cy);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{WorldDraft, build_world};
    use serde_json::json;

    /// An open field with a wall across column 10 (rows 0-7), an exit on the east edge and
    /// the party at (5, 5).
    fn field() -> World {
        let mut map = vec![".".repeat(MAP_W); MAP_H];
        for row in map.iter_mut().take(8) {
            row.replace_range(10..11, "#");
        }
        let draft: WorldDraft = serde_json::from_value(json!({
            "location": {
                "id": "field", "legend": { ".": "grass", "#": "stone_wall" }, "map": map, "spawn": [5, 5],
                "exits": [{ "x": 29, "y": 12, "to": "east", "label": "East" }]
            }
        }))
        .unwrap();
        build_world(&draft, "", "English").unwrap().0
    }

    #[test]
    fn open_ground_is_walked_freely() {
        let mut w = field();
        assert!(w.walk(10.0, -6.0));
        assert_eq!((w.player.x, w.player.y), (98.0, 82.0));
    }

    #[test]
    fn a_wall_stops_the_feet_at_its_face() {
        let mut w = field();
        for _ in 0..100 {
            w.walk(2.0, 0.0);
        }
        // The wall's west face is at x = 160; the feet are 4 wide on each side.
        assert_eq!(w.player.x, 155.0);
        assert!(!w.walk(2.0, 0.0));
    }

    #[test]
    fn walking_diagonally_into_a_wall_slides_along_it() {
        let mut w = field();
        for _ in 0..100 {
            w.walk(2.0, 0.0);
        }
        let y = w.player.y;
        assert!(w.walk(2.0, 2.0));
        assert_eq!((w.player.x, w.player.y), (155.0, y + 2.0));
    }

    #[test]
    fn the_edge_of_the_map_is_a_wall() {
        let mut w = field();
        for _ in 0..200 {
            w.walk(-3.0, -3.0);
        }
        assert_eq!((w.player.x, w.player.y), (FEET.0, FEET.1));
    }

    #[test]
    fn standing_on_an_exit_is_noticed_and_nearby_exits_are_found() {
        let mut w = field();
        assert!(w.exit_here().is_none() && w.exit_near(24.0).is_none());
        w.player = Player::in_cell(27, 12);
        assert_eq!(w.exit_near(40.0).map(|e| e.to.as_str()), Some("east"));
        assert!(w.exit_near(20.0).is_none());
        w.player = Player::in_cell(29, 12);
        assert_eq!(w.exit_here().map(|e| e.to.as_str()), Some("east"));
    }

    #[test]
    fn stepping_back_leaves_the_exit_for_free_ground() {
        let mut w = field();
        w.player = Player::in_cell(29, 12);
        w.step_back();
        assert!(w.exit_here().is_none());
        let (x, y) = w.player.cell();
        assert!(!w.here().blocked(x, y, &w.tiles));
    }
}

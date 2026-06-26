use super::vec2d::Vec2d;

#[derive(Copy, Clone)]
pub struct Rect {
    pub position: Vec2d,
    pub size: Vec2d,
}

impl Rect {
    pub fn new(position: Vec2d, size: Vec2d) -> Rect {
        Rect { position, size }
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.position.x < other.position.x + other.size.x
            && self.position.x + self.size.x > other.position.x
            && self.position.y < other.position.y + other.size.y
            && self.position.y + self.size.y > other.position.y
    }

    pub fn shrink(&self, size: f32) -> Rect {
        Rect {
            position: Vec2d::new(self.position.x + size, self.position.y + size),
            size: Vec2d::new(self.size.x - size * 2.0, self.size.y - size * 2.0),
        }
    }
}

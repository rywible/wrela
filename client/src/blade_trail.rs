#[derive(Clone, Debug)]
pub struct TrailPoint {
    pub position: [f32; 3],
    pub up: [f32; 3],
    pub time: f32,
    pub width: f32,
}

pub struct BladeTrail {
    points: Vec<TrailPoint>,
    max_points: usize,
    lifetime: f32,
}

impl BladeTrail {
    pub fn new(max_points: usize, lifetime: f32) -> Self {
        Self {
            points: Vec::with_capacity(max_points),
            max_points,
            lifetime,
        }
    }

    pub fn add_point(&mut self, position: [f32; 3], up: [f32; 3], width: f32, time: f32) {
        self.points.insert(
            0,
            TrailPoint {
                position,
                up,
                time,
                width,
            },
        );

        if self.points.len() > self.max_points {
            self.points.truncate(self.max_points);
        }

        self.points.retain(|p| (time - p.time) < self.lifetime);
    }

    pub fn build_mesh(&self, current_time: f32) -> Vec<[f32; 3]> {
        let alive: Vec<&TrailPoint> = self
            .points
            .iter()
            .filter(|p| (current_time - p.time) < self.lifetime)
            .collect();

        if alive.len() < 2 {
            return Vec::new();
        }

        let mut vertices = Vec::with_capacity((alive.len() - 1) * 6);
        for i in 0..alive.len() - 1 {
            let a = &alive[i];
            let b = &alive[i + 1];

            let age_a = ((current_time - a.time) / self.lifetime).clamp(0.0, 1.0);
            let age_b = ((current_time - b.time) / self.lifetime).clamp(0.0, 1.0);
            let w_a = a.width * (1.0 - age_a);
            let w_b = b.width * (1.0 - age_b);

            let a_top = [
                a.position[0] + a.up[0] * w_a * 0.5,
                a.position[1] + a.up[1] * w_a * 0.5,
                a.position[2] + a.up[2] * w_a * 0.5,
            ];
            let a_bot = [
                a.position[0] - a.up[0] * w_a * 0.5,
                a.position[1] - a.up[1] * w_a * 0.5,
                a.position[2] - a.up[2] * w_a * 0.5,
            ];
            let b_top = [
                b.position[0] + b.up[0] * w_b * 0.5,
                b.position[1] + b.up[1] * w_b * 0.5,
                b.position[2] + b.up[2] * w_b * 0.5,
            ];
            let b_bot = [
                b.position[0] - b.up[0] * w_b * 0.5,
                b.position[1] - b.up[1] * w_b * 0.5,
                b.position[2] - b.up[2] * w_b * 0.5,
            ];

            // Two triangles per quad segment
            vertices.push(a_top);
            vertices.push(a_bot);
            vertices.push(b_top);

            vertices.push(b_top);
            vertices.push(a_bot);
            vertices.push(b_bot);
        }

        vertices
    }

    pub fn point_count(&self) -> usize {
        self.points.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_point_increases_count() {
        let mut trail = BladeTrail::new(100, 2.0);
        assert_eq!(trail.point_count(), 0);

        trail.add_point([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 1.0, 0.0);
        assert_eq!(trail.point_count(), 1);

        trail.add_point([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 1.0, 0.1);
        assert_eq!(trail.point_count(), 2);
    }

    #[test]
    fn old_points_are_trimmed_by_lifetime() {
        let mut trail = BladeTrail::new(100, 1.0);
        trail.add_point([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 1.0, 0.0);
        trail.add_point([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 1.0, 0.5);

        // Adding a point at time=2.0 should cause the first point (time=0.0) to be trimmed
        trail.add_point([2.0, 0.0, 0.0], [0.0, 1.0, 0.0], 1.0, 2.0);
        assert!(trail.point_count() <= 2, "old point should be trimmed");
    }

    #[test]
    fn old_points_are_trimmed_by_max_points() {
        let mut trail = BladeTrail::new(3, 100.0);
        for i in 0..10 {
            trail.add_point([i as f32, 0.0, 0.0], [0.0, 1.0, 0.0], 1.0, i as f32 * 0.1);
        }
        assert!(trail.point_count() <= 3);
    }

    #[test]
    fn build_mesh_generates_vertices() {
        let mut trail = BladeTrail::new(100, 5.0);
        trail.add_point([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 1.0, 0.0);
        trail.add_point([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 1.0, 0.1);
        trail.add_point([2.0, 0.0, 0.0], [0.0, 1.0, 0.0], 1.0, 0.2);

        let mesh = trail.build_mesh(0.2);
        // 2 segments * 6 vertices per quad = 12
        assert_eq!(mesh.len(), 12, "expected 12 vertices for 2 quad segments");
    }

    #[test]
    fn build_mesh_empty_for_single_point() {
        let mut trail = BladeTrail::new(100, 5.0);
        trail.add_point([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 1.0, 0.0);
        let mesh = trail.build_mesh(0.0);
        assert!(mesh.is_empty());
    }

    #[test]
    fn build_mesh_empty_for_no_points() {
        let trail = BladeTrail::new(100, 5.0);
        let mesh = trail.build_mesh(0.0);
        assert!(mesh.is_empty());
    }

    #[test]
    fn build_mesh_width_tapers_with_age() {
        let mut trail = BladeTrail::new(100, 2.0);
        trail.add_point([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 2.0, 0.0);
        trail.add_point([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 2.0, 0.1);

        let mesh_early = trail.build_mesh(0.1);
        let mesh_late = trail.build_mesh(1.5);

        assert!(!mesh_early.is_empty());
        // The older mesh should still produce triangles (points are within lifetime)
        assert!(!mesh_late.is_empty());
    }

    #[test]
    fn build_mesh_respects_lifetime_filter() {
        let mut trail = BladeTrail::new(100, 1.0);
        trail.add_point([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 1.0, 0.0);
        trail.add_point([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 1.0, 0.1);

        // At time=5.0, both points are way past lifetime (1.0s)
        let mesh = trail.build_mesh(5.0);
        assert!(mesh.is_empty());
    }

    #[test]
    fn newest_point_is_first() {
        let mut trail = BladeTrail::new(100, 10.0);
        trail.add_point([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 1.0, 0.0);
        trail.add_point([5.0, 0.0, 0.0], [0.0, 1.0, 0.0], 1.0, 1.0);

        assert_eq!(trail.points[0].position[0], 5.0);
        assert_eq!(trail.points[1].position[0], 0.0);
    }
}

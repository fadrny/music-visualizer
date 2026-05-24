use glam::{Mat4, Vec3};

#[derive(Clone, Copy)]
pub struct Camera {
    pub pos: Vec3,
    pub azimuth: f32,
    pub zenith: f32,
}

impl Camera {
    pub fn new() -> Self {
        Self { pos: Vec3::ZERO, azimuth: 0.0, zenith: 0.0 }
    }

    pub fn with_position(mut self, p: Vec3) -> Self { self.pos = p; self }
    pub fn with_azimuth(mut self, a: f32) -> Self { self.azimuth = a; self }
    pub fn with_zenith(mut self, z: f32) -> Self { self.zenith = z.clamp(-std::f32::consts::FRAC_PI_2 + 0.001, std::f32::consts::FRAC_PI_2 - 0.001); self }

    pub fn add_azimuth(self, d: f32) -> Self { self.with_azimuth(self.azimuth + d) }
    pub fn add_zenith(self, d: f32) -> Self { self.with_zenith(self.zenith + d) }

    fn forward_dir(&self) -> Vec3 {
        Vec3::new(
            self.zenith.cos() * self.azimuth.sin(),
            self.zenith.sin(),
            self.zenith.cos() * -self.azimuth.cos(),
        )
    }
    fn right_dir(&self) -> Vec3 {
        Vec3::new(self.azimuth.cos(), 0.0, self.azimuth.sin())
    }

    pub fn forward(self, d: f32) -> Self { Self { pos: self.pos + self.forward_dir() * d, ..self } }
    pub fn backward(self, d: f32) -> Self { Self { pos: self.pos - self.forward_dir() * d, ..self } }
    pub fn right(self, d: f32) -> Self { Self { pos: self.pos + self.right_dir() * d, ..self } }
    pub fn left(self, d: f32) -> Self { Self { pos: self.pos - self.right_dir() * d, ..self } }

    pub fn view_matrix(&self) -> Mat4 {
        let target = self.pos + self.forward_dir();
        Mat4::look_at_rh(self.pos, target, Vec3::Y)
    }
}

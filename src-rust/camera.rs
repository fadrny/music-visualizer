use glam::{Mat4, Vec3};

// 1:1 port of transforms.Camera (Z-up convention).
// viewVector = (cos(a)cos(z), sin(a)cos(z), sin(z))
// upVector   = (cos(a)cos(z+pi/2), sin(a)cos(z+pi/2), sin(z+pi/2))
#[derive(Clone, Copy)]
pub struct Camera {
    pub pos: Vec3,
    pub azimuth: f32,
    pub zenith: f32,
    pub radius: f32,
    pub first_person: bool,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            pos: Vec3::ZERO,
            azimuth: 0.0,
            zenith: 0.0,
            radius: 1.0,
            first_person: true,
        }
    }

    pub fn with_position(mut self, p: Vec3) -> Self { self.pos = p; self }
    pub fn with_azimuth(mut self, a: f32) -> Self { self.azimuth = a; self }
    pub fn with_zenith(mut self, z: f32) -> Self {
        let lim = std::f32::consts::FRAC_PI_2;
        self.zenith = z.clamp(-lim, lim);
        self
    }

    pub fn add_azimuth(self, d: f32) -> Self { self.with_azimuth(self.azimuth + d) }
    pub fn add_zenith(self, d: f32) -> Self { self.with_zenith(self.zenith + d) }

    fn view_vector(&self) -> Vec3 {
        Vec3::new(
            self.azimuth.cos() * self.zenith.cos(),
            self.azimuth.sin() * self.zenith.cos(),
            self.zenith.sin(),
        )
    }

    fn up_vector(&self) -> Vec3 {
        let z2 = self.zenith + std::f32::consts::FRAC_PI_2;
        Vec3::new(
            self.azimuth.cos() * z2.cos(),
            self.azimuth.sin() * z2.cos(),
            z2.sin(),
        )
    }

    pub fn forward(self, speed: f32) -> Self {
        Self { pos: self.pos + self.view_vector() * speed, ..self }
    }
    pub fn backward(self, speed: f32) -> Self { self.forward(-speed) }

    pub fn right(self, speed: f32) -> Self {
        let a = self.azimuth - std::f32::consts::FRAC_PI_2;
        Self { pos: self.pos + Vec3::new(a.cos(), a.sin(), 0.0) * speed, ..self }
    }
    pub fn left(self, speed: f32) -> Self { self.right(-speed) }

    pub fn view_matrix(&self) -> Mat4 {
        let v = self.view_vector();
        let up = self.up_vector();
        let (eye, target) = if self.first_person {
            (self.pos, self.pos + v * self.radius)
        } else {
            (self.pos - v * self.radius, self.pos)
        };
        Mat4::look_at_rh(eye, target, up)
    }
}

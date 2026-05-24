use glow::HasContext;

pub const MAX_RADIUS: f32 = 5.0;
const RINGS: usize = 128;
const SECTORS: usize = 256;

pub struct Disc {
    pub vao: glow::VertexArray,
    #[allow(dead_code)] pub vbo: glow::Buffer,
    #[allow(dead_code)] pub ebo: glow::Buffer,
    pub index_count: i32,
}

impl Disc {
    pub unsafe fn new(gl: &glow::Context, program: glow::Program) -> Self {
        let mut verts: Vec<f32> = Vec::with_capacity(RINGS * SECTORS * 5);
        for r in 0..RINGS {
            let radius = r as f32 / (RINGS - 1) as f32 * MAX_RADIUS;
            for s in 0..SECTORS {
                let angle = s as f32 / SECTORS as f32 * std::f32::consts::TAU;
                verts.push(radius * angle.cos());
                verts.push(0.0);
                verts.push(radius * angle.sin());
                verts.push(radius / MAX_RADIUS);
                verts.push(s as f32 / SECTORS as f32);
            }
        }

        let mut indices: Vec<u32> = Vec::with_capacity((RINGS - 1) * SECTORS * 6);
        for r in 0..(RINGS - 1) {
            for s in 0..SECTORS {
                let next_s = (s + 1) % SECTORS;
                let cur = (r * SECTORS + s) as u32;
                let cur_next = (r * SECTORS + next_s) as u32;
                let ring = ((r + 1) * SECTORS + s) as u32;
                let ring_next = ((r + 1) * SECTORS + next_s) as u32;
                indices.extend_from_slice(&[cur, cur_next, ring, cur_next, ring_next, ring]);
            }
        }

        println!(
            "Disc created: {} rings x {} sectors ({} triangles)",
            RINGS, SECTORS, indices.len() / 3
        );

        let vao = gl.create_vertex_array().unwrap();
        let vbo = gl.create_buffer().unwrap();
        let ebo = gl.create_buffer().unwrap();

        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            bytemuck_cast_slice_f32(&verts),
            glow::STATIC_DRAW,
        );
        gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
        gl.buffer_data_u8_slice(
            glow::ELEMENT_ARRAY_BUFFER,
            bytemuck_cast_slice_u32(&indices),
            glow::STATIC_DRAW,
        );

        let stride = 5 * std::mem::size_of::<f32>() as i32;
        let pos_loc = gl.get_attrib_location(program, "inPosition").unwrap_or(0);
        let uv_loc = gl.get_attrib_location(program, "inUV").unwrap_or(1);
        gl.enable_vertex_attrib_array(pos_loc);
        gl.vertex_attrib_pointer_f32(pos_loc, 3, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(uv_loc);
        gl.vertex_attrib_pointer_f32(uv_loc, 2, glow::FLOAT, false, stride, 3 * std::mem::size_of::<f32>() as i32);

        gl.bind_vertex_array(None);

        Self { vao, vbo, ebo, index_count: indices.len() as i32 }
    }

    pub unsafe fn draw(&self, gl: &glow::Context) {
        gl.bind_vertex_array(Some(self.vao));
        gl.draw_elements(glow::TRIANGLES, self.index_count, glow::UNSIGNED_INT, 0);
        gl.bind_vertex_array(None);
    }
}

fn bytemuck_cast_slice_f32(s: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
}
fn bytemuck_cast_slice_u32(s: &[u32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
}

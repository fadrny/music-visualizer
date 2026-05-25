use ab_glyph::{Font, FontArc, Glyph, PxScale, ScaleFont};
use glow::HasContext;
use std::collections::HashMap;

const VS: &str = r#"#version 330 core
in vec2 inPos;
in vec2 inUV;
uniform vec2 uScreen;
out vec2 vUV;
void main() {
    // inPos is in pixel coords, top-left origin
    vec2 ndc;
    ndc.x = inPos.x / uScreen.x * 2.0 - 1.0;
    ndc.y = 1.0 - inPos.y / uScreen.y * 2.0;
    vUV = inUV;
    gl_Position = vec4(ndc, 0.0, 1.0);
}"#;

const FS: &str = r#"#version 330 core
in vec2 vUV;
uniform sampler2D uTex;
uniform vec3 uColor;
out vec4 FragColor;
void main() {
    float a = texture(uTex, vUV).r;
    if (a < 0.01) discard;
    FragColor = vec4(uColor, a);
}"#;

struct GlyphInfo {
    u0: f32, v0: f32, u1: f32, v1: f32,
    px_w: f32, px_h: f32,
    bearing_x: f32, bearing_y: f32, // px_bounds.min relative to baseline
    advance: f32,
}

#[allow(dead_code)]
pub struct TextRenderer {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    texture: glow::Texture,
    atlas_w: u32,
    atlas_h: u32,
    glyphs: HashMap<char, GlyphInfo>,
    line_height: f32,
    ascent: f32,
    space_advance: f32,
    queue: Vec<f32>,
    scale: PxScale,
    font: FontArc,
    width: i32,
    height: i32,
}

impl TextRenderer {
    pub unsafe fn new(gl: &glow::Context, width: i32, height: i32) -> Self {
        let font = load_font();
        let scale = PxScale::from(14.0);

        let (texture, atlas_w, atlas_h, glyphs, line_height, ascent, space_advance) =
            build_atlas(gl, &font, scale);

        let program = crate::shader::load_program(gl, VS, FS);

        let vao = gl.create_vertex_array().unwrap();
        let vbo = gl.create_buffer().unwrap();
        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        let stride = 4 * std::mem::size_of::<f32>() as i32;
        let pos = gl.get_attrib_location(program, "inPos").unwrap_or(0);
        let uv = gl.get_attrib_location(program, "inUV").unwrap_or(1);
        gl.enable_vertex_attrib_array(pos);
        gl.vertex_attrib_pointer_f32(pos, 2, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(uv);
        gl.vertex_attrib_pointer_f32(uv, 2, glow::FLOAT, false, stride, 2 * std::mem::size_of::<f32>() as i32);
        gl.bind_vertex_array(None);

        Self {
            program, vao, vbo, texture, atlas_w, atlas_h,
            glyphs, line_height, ascent, space_advance,
            queue: Vec::new(),
            scale, font, width, height,
        }
    }

    pub fn resize(&mut self, w: i32, h: i32) { self.width = w; self.height = h; }

    pub fn add_str(&mut self, x: i32, y: i32, text: &str) {
        let mut pen_x = x as f32;
        let baseline_y = y as f32;
        for ch in text.chars() {
            if ch == ' ' {
                pen_x += self.space_advance;
                continue;
            }
            // Use a fallback glyph for characters we didn't bake.
            let g = match self.glyphs.get(&ch) {
                Some(g) => g,
                None => match self.glyphs.get(&'?') {
                    Some(g) => g,
                    None => { pen_x += self.space_advance; continue; }
                },
            };
            let x0 = pen_x + g.bearing_x;
            let y0 = baseline_y + g.bearing_y; // bearing_y is offset from baseline (negative = above)
            let x1 = x0 + g.px_w;
            let y1 = y0 + g.px_h;
            let (u0, v0, u1, v1) = (g.u0, g.v0, g.u1, g.v1);

            // 2 triangles
            self.queue.extend_from_slice(&[
                x0, y0, u0, v0,
                x1, y0, u1, v0,
                x0, y1, u0, v1,
                x1, y0, u1, v0,
                x1, y1, u1, v1,
                x0, y1, u0, v1,
            ]);
            pen_x += g.advance;
        }
    }

    pub unsafe fn flush(&mut self, gl: &glow::Context) {
        if self.queue.is_empty() { return; }

        gl.disable(glow::DEPTH_TEST);
        gl.enable(glow::BLEND);
        gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

        gl.use_program(Some(self.program));
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
        let loc_tex = gl.get_uniform_location(self.program, "uTex");
        gl.uniform_1_i32(loc_tex.as_ref(), 0);
        let loc_screen = gl.get_uniform_location(self.program, "uScreen");
        gl.uniform_2_f32(loc_screen.as_ref(), self.width as f32, self.height as f32);
        let loc_color = gl.get_uniform_location(self.program, "uColor");
        gl.uniform_3_f32(loc_color.as_ref(), 1.0, 1.0, 1.0);

        gl.bind_vertex_array(Some(self.vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
        let bytes: &[u8] = std::slice::from_raw_parts(
            self.queue.as_ptr() as *const u8,
            std::mem::size_of_val(self.queue.as_slice()),
        );
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::DYNAMIC_DRAW);
        gl.draw_arrays(glow::TRIANGLES, 0, (self.queue.len() / 4) as i32);
        gl.bind_vertex_array(None);
        gl.disable(glow::BLEND);
        gl.enable(glow::DEPTH_TEST);

        self.queue.clear();
    }
}

unsafe fn build_atlas(
    gl: &glow::Context,
    font: &FontArc,
    scale: PxScale,
) -> (glow::Texture, u32, u32, HashMap<char, GlyphInfo>, f32, f32, f32) {
    let scaled = font.as_scaled(scale);
    let ascent = scaled.ascent();
    let line_height = scaled.height() + scaled.line_gap();
    let space_advance = scaled.h_advance(font.glyph_id(' '));

    let atlas_w: u32 = 1024;
    let mut atlas_h: u32 = 256;
    let mut pixels = vec![0u8; (atlas_w * atlas_h) as usize];
    let mut x: u32 = 1;
    let mut y: u32 = 1;
    let mut row_h: u32 = 0;
    let mut glyphs: HashMap<char, GlyphInfo> = HashMap::new();

    // ASCII printable (33..127) + Latin-1 Supplement (160..256) + Latin Extended-A (256..384, covers Czech/Polish/etc) + a few common punctuation chars.
    let ranges: &[std::ops::Range<u32>] = &[33..127, 160..384, 0x2010..0x2020];
    let codes = ranges.iter().flat_map(|r| r.clone());

    for code in codes {
        let Some(ch) = char::from_u32(code) else { continue; };
        let glyph: Glyph = font.glyph_id(ch).with_scale(scale);
        let advance = scaled.h_advance(font.glyph_id(ch));
        let Some(outline) = font.outline_glyph(glyph) else {
            glyphs.insert(ch, GlyphInfo {
                u0: 0.0, v0: 0.0, u1: 0.0, v1: 0.0,
                px_w: 0.0, px_h: 0.0, bearing_x: 0.0, bearing_y: 0.0, advance,
            });
            continue;
        };
        let bounds = outline.px_bounds();
        let gw = bounds.width().ceil() as u32;
        let gh = bounds.height().ceil() as u32;

        if x + gw + 1 >= atlas_w {
            x = 1;
            y += row_h + 1;
            row_h = 0;
        }
        while y + gh + 1 >= atlas_h {
            atlas_h *= 2;
            let mut new_pix = vec![0u8; (atlas_w * atlas_h) as usize];
            new_pix[..pixels.len()].copy_from_slice(&pixels);
            pixels = new_pix;
        }

        outline.draw(|px, py, c| {
            let pi = (y + py) * atlas_w + (x + px);
            pixels[pi as usize] = (c * 255.0).clamp(0.0, 255.0) as u8;
        });

        let u0 = x as f32 / atlas_w as f32;
        let v0 = y as f32 / atlas_h as f32;
        let u1 = (x + gw) as f32 / atlas_w as f32;
        let v1 = (y + gh) as f32 / atlas_h as f32;

        glyphs.insert(ch, GlyphInfo {
            u0, v0, u1, v1,
            px_w: gw as f32, px_h: gh as f32,
            bearing_x: bounds.min.x,
            bearing_y: bounds.min.y, // negative => above baseline
            advance,
        });

        x += gw + 1;
        if gh > row_h { row_h = gh; }
    }

    let texture = gl.create_texture().unwrap();
    gl.bind_texture(glow::TEXTURE_2D, Some(texture));
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
    gl.tex_image_2d(
        glow::TEXTURE_2D, 0, glow::R8 as i32,
        atlas_w as i32, atlas_h as i32, 0,
        glow::RED, glow::UNSIGNED_BYTE, Some(&pixels),
    );
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);

    (texture, atlas_w, atlas_h, glyphs, line_height, ascent, space_advance)
}

fn load_font() -> FontArc {
    // Try common Windows fonts; otherwise fall back to a tiny embedded.
    let candidates = [
        r"C:\Windows\Fonts\segoeui.ttf",
        r"C:\Windows\Fonts\consola.ttf",
        r"C:\Windows\Fonts\arial.ttf",
        r"C:\Windows\Fonts\tahoma.ttf",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(f) = FontArc::try_from_vec(bytes) {
                return f;
            }
        }
    }
    panic!("No usable system font found");
}

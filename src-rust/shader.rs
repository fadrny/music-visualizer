use glow::HasContext;

pub unsafe fn load_program(gl: &glow::Context, vert_src: &str, frag_src: &str) -> glow::Program {
    let program = gl.create_program().expect("create program");

    let vs = compile(gl, glow::VERTEX_SHADER, vert_src);
    let fs = compile(gl, glow::FRAGMENT_SHADER, frag_src);
    gl.attach_shader(program, vs);
    gl.attach_shader(program, fs);
    gl.link_program(program);
    if !gl.get_program_link_status(program) {
        panic!("program link error: {}", gl.get_program_info_log(program));
    }
    gl.detach_shader(program, vs);
    gl.detach_shader(program, fs);
    gl.delete_shader(vs);
    gl.delete_shader(fs);
    program
}

unsafe fn compile(gl: &glow::Context, kind: u32, src: &str) -> glow::Shader {
    let s = gl.create_shader(kind).expect("create shader");
    gl.shader_source(s, src);
    gl.compile_shader(s);
    if !gl.get_shader_compile_status(s) {
        panic!("shader compile error: {}", gl.get_shader_info_log(s));
    }
    s
}

mod audio;
mod camera;
mod disc;
mod shader;
mod spotify;
mod text_renderer;

use std::ffi::CString;
use std::num::NonZeroU32;
use std::time::Instant;

use glam::{Mat4, Vec3};
use glow::HasContext;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, Version};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{Surface, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use raw_window_handle::HasRawWindowHandle;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, Event, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::WindowBuilder;

use audio::AudioFFT;
use camera::Camera;
use disc::{Disc, MAX_RADIUS};
use spotify::SpotifyClient;
use text_renderer::TextRenderer;

const VERT_SRC: &str = include_str!("shaders/terrain.vert");
const FRAG_SRC: &str = include_str!("shaders/terrain.frag");

struct App {
    width: i32,
    height: i32,
    cam: Camera,
    proj: Mat4,
    mouse_pressed: bool,
    last_mouse: (f64, f64),
    wireframe: bool,
    amplitude: f32,

    gl: glow::Context,
    program: glow::Program,
    disc: Disc,
    text: TextRenderer,

    loc_mat: Option<glow::UniformLocation>,
    loc_fft: Option<glow::UniformLocation>,
    loc_amp: Option<glow::UniformLocation>,
    loc_time: Option<glow::UniformLocation>,
    loc_album: Option<glow::UniformLocation>,
    loc_has_album: Option<glow::UniformLocation>,
    loc_maxr: Option<glow::UniformLocation>,

    audio: AudioFFT,
    spotify: SpotifyClient,
    spotify_mode: bool,
    album_tex: Option<glow::Texture>,
    has_album: bool,

    start_time: Instant,
}

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let window_builder = WindowBuilder::new()
        .with_title("Vinyl music visualizer")
        .with_inner_size(PhysicalSize::new(1280, 720));

    let template = ConfigTemplateBuilder::new().with_depth_size(24);
    let display_builder = DisplayBuilder::new().with_window_builder(Some(window_builder));

    let (window, gl_config) = display_builder
        .build(&event_loop, template, |configs| {
            configs.reduce(|acc, c| if c.num_samples() > acc.num_samples() { c } else { acc }).unwrap()
        })
        .expect("create window+config");
    let window = window.expect("window");

    let raw_window_handle = Some(window.raw_window_handle());
    let gl_display = gl_config.display();

    let context_attribs = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .build(raw_window_handle);

    let not_current = unsafe {
        gl_display.create_context(&gl_config, &context_attribs).expect("gl context")
    };

    let attrs = window.build_surface_attributes(Default::default());
    let surface: Surface<WindowSurface> = unsafe {
        gl_display.create_window_surface(&gl_config, &attrs).expect("surface")
    };
    let gl_context = not_current.make_current(&surface).expect("make current");
    let _ = surface.set_swap_interval(&gl_context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()));

    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            let c = CString::new(s).unwrap();
            gl_display.get_proc_address(&c) as *const _
        })
    };

    let size = window.inner_size();
    let (width, height) = (size.width as i32, size.height as i32);

    unsafe {
        gl.viewport(0, 0, width, height);
        gl.clear_color(0.01, 0.0, 0.03, 1.0);
        gl.disable(glow::CULL_FACE);
        gl.enable(glow::DEPTH_TEST);
        gl.enable(glow::LINE_SMOOTH);
        print_gl_info(&gl);
    }

    let program = unsafe { shader::load_program(&gl, VERT_SRC, FRAG_SRC) };
    unsafe { gl.use_program(Some(program)); }

    let disc = unsafe { Disc::new(&gl, program) };
    let text = unsafe { TextRenderer::new(&gl, width, height) };

    let (loc_mat, loc_fft, loc_amp, loc_time, loc_album, loc_has_album, loc_maxr) = unsafe {(
        gl.get_uniform_location(program, "mat"),
        gl.get_uniform_location(program, "uFFT"),
        gl.get_uniform_location(program, "uAmplitude"),
        gl.get_uniform_location(program, "uTime"),
        gl.get_uniform_location(program, "uAlbumArt"),
        gl.get_uniform_location(program, "uHasAlbumArt"),
        gl.get_uniform_location(program, "uMaxRadius"),
    )};

    let mut audio = AudioFFT::new();
    audio.start();

    let cam = Camera::new()
        .with_position(Vec3::new(4.0, 6.0, 3.0))
        .with_azimuth(std::f32::consts::PI * 1.25)
        .with_zenith(std::f32::consts::PI * -0.20);

    let proj = Mat4::perspective_rh_gl(
        std::f32::consts::FRAC_PI_4,
        width as f32 / height as f32,
        0.01, 1000.0,
    );

    let mut app = App {
        width, height, cam, proj,
        mouse_pressed: false, last_mouse: (0.0, 0.0),
        wireframe: false, amplitude: 1.5,
        gl, program, disc, text,
        loc_mat, loc_fft, loc_amp, loc_time, loc_album, loc_has_album, loc_maxr,
        audio, spotify: SpotifyClient::new(),
        spotify_mode: false, album_tex: None, has_album: false,
        start_time: Instant::now(),
    };

    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::Resized(sz) => {
                    if sz.width > 0 && sz.height > 0 {
                        app.width = sz.width as i32;
                        app.height = sz.height as i32;
                        surface.resize(&gl_context,
                            NonZeroU32::new(sz.width).unwrap(),
                            NonZeroU32::new(sz.height).unwrap(),
                        );
                        unsafe { app.gl.viewport(0, 0, app.width, app.height); }
                        // Java: aspect = height/width (note: passes ratio that way)
                        app.proj = Mat4::perspective_rh_gl(
                            std::f32::consts::FRAC_PI_4,
                            app.width as f32 / app.height as f32,
                            0.01, 1000.0,
                        );
                        app.text.resize(app.width, app.height);
                    }
                }
                WindowEvent::KeyboardInput { event: KeyEvent { physical_key, state, .. }, .. } => {
                    if state == ElementState::Pressed {
                        if let PhysicalKey::Code(code) = physical_key {
                            match code {
                                KeyCode::Escape => elwt.exit(),
                                KeyCode::KeyW => app.cam = app.cam.forward(0.2),
                                KeyCode::KeyS => app.cam = app.cam.backward(0.2),
                                KeyCode::KeyA => app.cam = app.cam.left(0.2),
                                KeyCode::KeyD => app.cam = app.cam.right(0.2),
                                KeyCode::Tab => app.wireframe = !app.wireframe,
                                KeyCode::NumpadAdd | KeyCode::Equal => {
                                    app.amplitude = (app.amplitude + 0.1).min(5.0);
                                }
                                KeyCode::NumpadSubtract | KeyCode::Minus => {
                                    app.amplitude = (app.amplitude - 0.1).max(0.0);
                                }
                                KeyCode::KeyM => app.audio.next_device(),
                                KeyCode::KeyO => toggle_spotify(&mut app),
                                _ => {}
                            }
                        }
                    }
                }
                WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                    // Match Java behavior: seize the cursor position at press
                    // (and again at release) so the first drag delta is zero.
                    let was_pressed = app.mouse_pressed;
                    app.mouse_pressed = state == ElementState::Pressed;
                    if app.mouse_pressed != was_pressed {
                        // Reset drag anchor to current cursor position.
                        // last_mouse has been kept current by CursorMoved.
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    let (x, y) = (position.x, position.y);
                    if app.mouse_pressed {
                        let dx = (app.last_mouse.0 - x) as f32 / app.width as f32 * std::f32::consts::PI;
                        let dy = (app.last_mouse.1 - y) as f32 / app.width as f32 * std::f32::consts::PI;
                        app.cam = app.cam.add_azimuth(dx).add_zenith(dy);
                    }
                    app.last_mouse = (x, y);
                }
                WindowEvent::RedrawRequested => {
                    render(&mut app);
                    surface.swap_buffers(&gl_context).ok();
                }
                _ => {}
            },
            Event::AboutToWait => {
                window.request_redraw();
            }
            _ => {}
        }
    }).expect("event loop");
}

fn toggle_spotify(app: &mut App) {
    app.spotify_mode = !app.spotify_mode;
    if app.spotify_mode
        && !app.spotify.is_connected()
        && !app.spotify.is_auth_in_progress()
    {
        app.spotify.start_auth();
    }
    println!("[Spotify] Mode {}", if app.spotify_mode { "ON" } else { "OFF" });
}

fn render(app: &mut App) {
    let time = app.start_time.elapsed().as_secs_f32();

    // Spotify album art upload (needs &mut app) before we take an &app.gl borrow.
    if app.spotify_mode {
        if let Some(bytes) = app.spotify.consume_new_album_art() {
            unsafe { upload_album_art(app, &bytes); }
        }
    }

    unsafe {
        let gl = &app.gl;
        gl.viewport(0, 0, app.width, app.height);
        gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);

        gl.use_program(Some(app.program));

        let mvp = app.proj * app.cam.view_matrix();
        gl.uniform_matrix_4_f32_slice(app.loc_mat.as_ref(), false, &mvp.to_cols_array());

        let bins = app.audio.get_bins();
        gl.uniform_1_f32_slice(app.loc_fft.as_ref(), &bins);
        gl.uniform_1_f32(app.loc_amp.as_ref(), app.amplitude);
        gl.uniform_1_f32(app.loc_time.as_ref(), time);

        let show_art = app.spotify_mode && app.has_album && app.album_tex.is_some();
        if show_art {
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, app.album_tex);
            gl.uniform_1_i32(app.loc_album.as_ref(), 1);
            gl.uniform_1_i32(app.loc_has_album.as_ref(), 1);
        } else {
            gl.uniform_1_i32(app.loc_has_album.as_ref(), 0);
        }
        gl.uniform_1_f32(app.loc_maxr.as_ref(), MAX_RADIUS);

        if app.wireframe {
            gl.polygon_mode(glow::FRONT_AND_BACK, glow::LINE);
        } else {
            gl.polygon_mode(glow::FRONT_AND_BACK, glow::FILL);
        }

        app.disc.draw(gl);

        if show_art {
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE0);
        }
    }

    // HUD
    let text_line = format!(
        "WASD: move | Tab: wireframe [{}] | +/-: amp [{:.1}] | M: audio | O: spotify [{}]",
        if app.wireframe { "ON" } else { "OFF" },
        app.amplitude,
        if app.spotify_mode { "ON" } else { "OFF" },
    );
    app.text.add_str(3, 20, &text_line);
    let audio_line = format!("Audio: {}", app.audio.device_name());
    app.text.add_str(3, 35, &audio_line);

    if app.spotify_mode {
        if app.spotify.is_connected() && app.spotify.has_track() {
            let s = format!("+ {} - {}", app.spotify.track_name(), app.spotify.artist_name());
            app.text.add_str(3, app.height - 10, &s);
        } else if app.spotify.is_auth_in_progress() {
            app.text.add_str(3, app.height - 10, "Spotify: connecting...");
        } else if app.spotify.is_connected() {
            app.text.add_str(3, app.height - 10, "Spotify: nothing playing");
        }
    }

    app.text.add_str(app.width - 180, app.height - 3, "Marek Fadrny | PGRF2 2025/26");

    unsafe { app.text.flush(&app.gl); }
}

unsafe fn upload_album_art(app: &mut App, bytes: &[u8]) {
    let img = match image::load_from_memory(bytes) {
        Ok(i) => i.to_rgba8(),
        Err(e) => {
            eprintln!("[Spotify] Failed to decode album art: {}", e);
            return;
        }
    };
    let (w, h) = (img.width(), img.height());

    let gl = &app.gl;
    if let Some(t) = app.album_tex.take() { gl.delete_texture(t); }

    let tex = gl.create_texture().unwrap();
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
    gl.tex_image_2d(
        glow::TEXTURE_2D, 0, glow::RGBA as i32,
        w as i32, h as i32, 0,
        glow::RGBA, glow::UNSIGNED_BYTE, Some(img.as_raw()),
    );
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
    gl.bind_texture(glow::TEXTURE_2D, None);

    app.album_tex = Some(tex);
    app.has_album = true;
    println!("[Spotify] Album art texture uploaded [{}x{}]", w, h);
}

unsafe fn print_gl_info(gl: &glow::Context) {
    let vendor = gl.get_parameter_string(glow::VENDOR);
    let renderer = gl.get_parameter_string(glow::RENDERER);
    let version = gl.get_parameter_string(glow::VERSION);
    let glsl = gl.get_parameter_string(glow::SHADING_LANGUAGE_VERSION);
    println!("OpenGL vendor:   {}", vendor);
    println!("OpenGL renderer: {}", renderer);
    println!("OpenGL version:  {}", version);
    println!("GLSL version:    {}", glsl);
}

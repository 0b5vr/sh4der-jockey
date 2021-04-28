use crate::util::*;
use gl::types::*;
use imgui::im_str;
use lazy_static::lazy_static;
use sdl2::{
    event::{Event, WindowEvent},
    keyboard::{Keycode, Mod},
};
use std::{
    ffi::CString,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

mod midi;
mod pipeline;
mod stage;

pub use midi::*;
pub use pipeline::*;
pub use stage::*;

lazy_static! {
    static ref JOCKEY_TITLE: String = {
        format!(
            "Sh4derJockey (version {}-{})",
            env!("VERGEN_BUILD_SEMVER"),
            &env!("VERGEN_GIT_SHA")[0..7]
        )
    };
}

static mut FILE_CHANGE: AtomicBool = AtomicBool::new(false);

/// A struct for all the ugly internals.
pub struct MegaContext {
    pub event_pump: sdl2::EventPump,
    pub gl_context: sdl2::video::GLContext,
    pub imgui_sdl2: imgui_sdl2::ImguiSdl2,
    pub imgui: imgui::Context,
    pub renderer: imgui_opengl_renderer::Renderer,
    pub vao: GLuint,
    pub vbo: GLuint,
    pub watcher: notify::RecommendedWatcher,
    pub window: sdl2::video::Window,
}

/// A struct to keep the state of the tool.
///
/// This struct holds the render pipeline, as well as every type of context
/// required to keep the window alive. The main point of this struct is to
/// hide all the nasty details and keep the main function clean.
pub struct Jockey {
    pub beat_delta: RunningAverage<f32, 8>,
    pub ctx: MegaContext,
    pub done: bool,
    pub frame_perf: RunningAverage<f32, 128>,
    pub last_beat: Instant,
    pub last_build: Instant,
    pub last_frame: Instant,
    pub midi: Midi<8>,
    pub pipeline: Pipeline,
    pub start_time: Instant,
}

impl std::fmt::Debug for Jockey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(stringify!(Jockey))
            .field("vao", &self.ctx.vao)
            .field("vbo", &self.ctx.vbo)
            .field("pipeline", &self.pipeline)
            .finish()
    }
}

impl Jockey {
    /// Returns a string containing the name of the program, the current
    /// version and commit hash.
    pub fn title() -> String {
        JOCKEY_TITLE.clone()
    }

    /// Initializes the tool.
    ///
    /// This will spin up a SDL2 window, initialize Imgui,
    /// create a OpenGL context and more!
    pub fn init() -> Self {
        let sdl_context = sdl2::init().unwrap();
        let video = sdl_context.video().unwrap();

        {
            let gl_attr = video.gl_attr();
            gl_attr.set_context_profile(sdl2::video::GLProfile::Core);
            gl_attr.set_context_version(3, 0);
        }

        let title = Self::title();
        let window = video
            .window(&title, 1280, 720)
            .position_centered()
            .resizable()
            .opengl()
            .allow_highdpi()
            .build()
            .unwrap();

        let gl_context = window
            .gl_create_context()
            .expect("Couldn't create GL context");

        gl::load_with(|s| video.gl_get_proc_address(s) as _);

        let mut imgui = imgui::Context::create();
        imgui.set_ini_filename(None);

        let imgui_sdl2 = imgui_sdl2::ImguiSdl2::new(&mut imgui, &window);
        let renderer =
            imgui_opengl_renderer::Renderer::new(&mut imgui, |s| video.gl_get_proc_address(s) as _);
        let event_pump = sdl_context.event_pump().unwrap();

        let mut vao = 0;
        let mut vbo = 0;

        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
        }

        let pipeline = Pipeline::new();
        let last_build = Instant::now();
        let frame_perf = RunningAverage::new();

        #[rustfmt::skip]
        let mut watcher = notify::immediate_watcher(
            |_| unsafe { FILE_CHANGE.store(true, Ordering::Relaxed) }
        ).unwrap();

        notify::Watcher::watch(&mut watcher, ".", notify::RecursiveMode::Recursive).unwrap();

        let midi = Midi::new();

        let ctx = MegaContext {
            event_pump,
            gl_context,
            imgui_sdl2,
            imgui,
            renderer,
            vao,
            vbo,
            watcher,
            window,
        };

        let mut beat_delta = RunningAverage::new();
        beat_delta.buffer.fill(1.0);

        let start_time = Instant::now();
        let last_frame = start_time;
        let last_beat = start_time;

        let mut this = Self {
            beat_delta,
            ctx,
            done: false,
            frame_perf,
            last_beat,
            last_build,
            last_frame,
            midi,
            pipeline,
            start_time,
        };

        this.update_pipeline();
        this
    }

    /// Reload the render pipeline and replace the old one.
    ///
    /// This will load the `pipeline.json` from the specified file and
    /// attempt to read and compile all necessary shaders. If everything loaded
    /// successfully, the new Pipeline struct will stomp the old one.
    pub fn update_pipeline(&mut self) {
        let start_time = Instant::now();
        let update = match Pipeline::load(&self.ctx.window) {
            Ok(pl) => pl,
            Err(err) => {
                eprintln!("Failed to load pipeline:\n{}", err);
                return;
            }
        };

        self.pipeline = update;
        println!("\n{:?}\n", self.pipeline);

        let time = start_time.elapsed().as_secs_f64();
        println!("Build pipeline in {}ms", 1000.0 * time);
    }

    pub fn handle_events(&mut self) {
        self.midi.handle_input();

        let mut do_update_pipeline = unsafe { FILE_CHANGE.swap(false, Ordering::Relaxed) }
            && self.last_build.elapsed().as_millis() > 100;

        for event in self.ctx.event_pump.poll_iter() {
            self.ctx
                .imgui_sdl2
                .handle_event(&mut self.ctx.imgui, &event);

            if self.ctx.imgui_sdl2.ignore_event(&event) {
                continue;
            }

            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => self.done = true,

                Event::KeyDown {
                    keycode: Some(Keycode::Return),
                    keymod,
                    ..
                } if keymod & Mod::LCTRLMOD != Mod::NOMOD => do_update_pipeline = true,

                Event::Window {
                    win_event: WindowEvent::Resized(width, height),
                    ..
                } if !do_update_pipeline => {
                    self.pipeline.resize_buffers(width as _, height as _);
                }
                _ => {}
            }
        }

        // live shader reloading hype
        if do_update_pipeline {
            self.update_pipeline();
            self.last_build = Instant::now();
        }
    }

    /// Does all the OpenGL magic.
    ///
    /// This function iterates over all stages in the pipeline and renders
    /// them front to back. The only reason this function takes an `&mut self`
    /// is to record performance statistics.
    pub fn draw(&mut self) -> Option<()> {
        lazy_static! {
            static ref R_NAME: CString = CString::new("R").unwrap();
            static ref TIME_NAME: CString = CString::new("time").unwrap();
            static ref BEAT_NAME: CString = CString::new("beat").unwrap();
            static ref SLIDERS_NAME: CString = CString::new("sliders").unwrap();
            static ref BUTTONS_NAME: CString = CString::new("buttons").unwrap();
            static ref VERTEX_COUNT_NAME: CString = CString::new("vertexCount").unwrap();
            static ref OUT_COLOR_NAME: CString = CString::new("out_color").unwrap();
            static ref POSITION_NAME: CString = CString::new("position").unwrap();
        }

        // compute uniforms
        let (width, height) = self.ctx.window.size();
        let time = self.start_time.elapsed().as_secs_f32();
        let beat = self.last_beat.elapsed().as_secs_f32() / self.beat_delta.get();

        // render all shader stages
        for stage in self.pipeline.stages.iter_mut() {
            let stage_start = Instant::now();

            // get size of the render target
            let target_res = match stage.resolution() {
                Some([w, h, 0]) => (w, h),
                _ => (width, height),
            };

            unsafe {
                // Use shader program
                gl::UseProgram(stage.prog_id);

                // Add time, beat and resolution
                {
                    let r_loc = gl::GetUniformLocation(stage.prog_id, R_NAME.as_ptr());
                    let time_loc = gl::GetUniformLocation(stage.prog_id, TIME_NAME.as_ptr());
                    let beat_loc = gl::GetUniformLocation(stage.prog_id, BEAT_NAME.as_ptr());

                    gl::Uniform3f(r_loc, target_res.0 as _, target_res.1 as _, time);
                    gl::Uniform1f(time_loc, time);
                    gl::Uniform1f(beat_loc, beat);
                }

                // Add sliders and buttons
                {
                    let s_loc = gl::GetUniformLocation(stage.prog_id, SLIDERS_NAME.as_ptr());
                    let b_loc = gl::GetUniformLocation(stage.prog_id, BUTTONS_NAME.as_ptr());

                    let mut buttons = [0.0; 8];
                    for k in 0..buttons.len() {
                        buttons[k] = self.midi.buttons[k].elapsed().as_secs_f32();
                    }

                    gl::Uniform1fv(s_loc, self.midi.sliders.len() as _, &self.midi.sliders as _);
                    gl::Uniform1fv(b_loc, buttons.len() as _, &buttons as _);
                }

                // Add vertex count uniform
                if let StageKind::Vert { count, .. } = stage.kind {
                    let loc = gl::GetUniformLocation(stage.prog_id, VERTEX_COUNT_NAME.as_ptr());
                    gl::Uniform1f(loc, count as _);
                }

                // Add and bind uniform texture dependencies
                for (k, name) in stage.deps.iter().enumerate() {
                    let tex = self.pipeline.buffers.get(name).unwrap();
                    let loc = gl::GetUniformLocation(stage.prog_id, name.as_ptr());

                    gl::ActiveTexture(gl::TEXTURE0 + k as GLenum);
                    gl::BindTexture(gl::TEXTURE_2D, tex.id);
                    gl::BindImageTexture(0, tex.id, 0, gl::FALSE, 0, gl::WRITE_ONLY, gl::RGBA32F);
                    gl::Uniform1i(loc, k as _);
                }
            }

            match &stage.kind {
                StageKind::Comp { tex_dim, .. } => unsafe {
                    gl::DispatchCompute(tex_dim[0], tex_dim[1].max(1), tex_dim[2].max(1));
                    gl::MemoryBarrier(gl::SHADER_IMAGE_ACCESS_BARRIER_BIT);
                },
                _ => {
                    // get render target id
                    let (target_tex, target_fb) = if let Some(name) = &stage.target {
                        let tex = &self.pipeline.buffers[name];
                        if let TextureKind::FrameBuffer { fb, .. } = tex.kind {
                            (tex.id, fb)
                        } else {
                            panic!("No framebuffer for render target!")
                        }
                    } else {
                        (0, 0) // The screen is always id=0
                    };

                    unsafe {
                        // Specify render target
                        gl::BindFramebuffer(gl::FRAMEBUFFER, target_fb);
                        gl::Viewport(0, 0, target_res.0 as _, target_res.1 as _);

                        // Specify fragment shader color output
                        gl::BindFragDataLocation(stage.prog_id, 0, OUT_COLOR_NAME.as_ptr());

                        // Specify the layout of the vertex data
                        let pos_attr = gl::GetAttribLocation(stage.prog_id, POSITION_NAME.as_ptr());
                        gl::EnableVertexAttribArray(pos_attr as GLuint);
                        gl::VertexAttribPointer(
                            pos_attr as GLuint,
                            2,
                            gl::FLOAT,
                            gl::FALSE as GLboolean,
                            0,
                            std::ptr::null(),
                        );

                        // Draw stuff
                        if let StageKind::Vert { count, mode, .. } = stage.kind {
                            gl::ClearColor(0.0, 0.0, 0.0, 0.0);
                            gl::ClearDepth(1.0);
                            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);

                            gl::Enable(gl::DEPTH_TEST);
                            gl::DepthMask(gl::TRUE);
                            gl::DepthFunc(gl::LEQUAL);
                            gl::DepthRange(0.0, 1.0);

                            draw_anything(self.ctx.vao, count, mode)
                        } else {
                            draw_fullscreen_rect(self.ctx.vao);
                        }

                        // Generate mip maps
                        gl::BindTexture(gl::TEXTURE_2D, target_tex);
                        gl::GenerateMipmap(gl::TEXTURE_2D);
                    }
                }
            }
            // log render time
            let stage_time = stage_start.elapsed().as_secs_f32();
            stage.perf.push(1000.0 * stage_time);
        }

        Some(())
    }

    /// Wrapper function for all the imgui stuff.
    pub fn build_ui(&mut self) {
        self.ctx.imgui_sdl2.prepare_frame(
            self.ctx.imgui.io_mut(),
            &self.ctx.window,
            &self.ctx.event_pump.mouse_state(),
        );

        // tell imgui what time it is
        let now = Instant::now();
        let delta_time = (now - self.last_frame).as_secs_f32();
        self.ctx.imgui.io_mut().delta_time = delta_time;
        self.last_frame = now;

        // record frame time
        self.frame_perf.push(1000.0 * delta_time);
        let frame_ms = self.frame_perf.get();

        // title section
        let ui = self.ctx.imgui.frame();
        ui.text(&*JOCKEY_TITLE);
        ui.separator();

        // sliders
        for k in 0..self.midi.sliders.len() {
            let token = ui.push_id(k as i32);
            if ui.small_button(im_str!("bind")) {
                self.midi.auto_bind_slider(k);
            }
            token.pop(&ui);
            ui.same_line(0.0);
            let name = format!("slider{}", k);
            let cst = std::ffi::CString::new(name).unwrap();
            let ims = unsafe { imgui::ImStr::from_cstr_unchecked(&cst) };
            let slider = &mut self.midi.sliders[k];
            imgui::Slider::new(ims).range(0.0..=1.0).build(&ui, slider);
        }

        // buttons
        for k in 0..self.midi.buttons.len() {
            let token = ui.push_id(-(k as i32)-1);
            if ui.small_button(im_str!("bind")) {
                self.midi.auto_bind_button(k);
            }
            token.pop(&ui);
            ui.same_line(0.0);
            let name = format!("button{}", k);
            let cst = std::ffi::CString::new(name).unwrap();
            let ims = unsafe { imgui::ImStr::from_cstr_unchecked(&cst) };
            if ui.button(ims, [64.0, 18.0]) {
                self.midi.buttons[k] = Instant::now();
            }
            if k & 3 != 3 {
                ui.same_line(0.0)
            }
        }

        ui.separator();

        // beat sync
        if ui.button(im_str!("Tab here"), [128.0, 32.0]) {
            let delta = self.last_beat.elapsed().as_secs_f32();
            self.beat_delta.push(delta);
            self.last_beat = Instant::now();
        }
        ui.same_line(0.0);
        ui.text(format! {
            "BPM: {}\nCycle: {}", 60.0 / self.beat_delta.get(), self.beat_delta.index
        });

        ui.separator();

        // perf monitor
        ui.text(format!(
            "FPS: {:.2} ({:.2} ms)",
            1000.0 / frame_ms,
            frame_ms
        ));

        ui.plot_lines(im_str!("dt [ms]"), &self.frame_perf.buffer)
            .build();

        let mut stage_sum_ms = 0.0;
        for (k, stage) in self.pipeline.stages.iter().enumerate() {
            let stage_ms = stage.perf.get();
            stage_sum_ms += stage_ms;
            if let Some(tex_name) = stage.target.as_ref() {
                ui.text(format!(
                    "Stage {}: {:.4} ms (-> {:?})",
                    k, stage_ms, tex_name
                ));
            } else {
                ui.text(format!("Stage {}: {:.4} ms", k, stage_ms));
            }
        }

        ui.text(format!(
            "Total: {:.4} ms ({:.2}% stress)",
            stage_sum_ms,
            100.0 * stage_sum_ms / frame_ms
        ));

        // update ui
        self.ctx.imgui_sdl2.prepare_render(&ui, &self.ctx.window);
        self.ctx.renderer.render(ui);
    }
}

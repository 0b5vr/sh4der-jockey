use gl::types::*;
use std::ffi::CString;

mod average;
mod texture;

pub use average::*;
pub use texture::*;

const FULLSCREEN_RECT: [GLfloat; 12] = [
    -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0,
];

pub fn draw_fullscreen_rect(vao: GLuint) {
    unsafe {
        gl::BindVertexArray(vao);
        gl::BindBuffer(gl::ARRAY_BUFFER, vao);

        let data_size = FULLSCREEN_RECT.len() * std::mem::size_of::<GLfloat>();
        gl::BufferData(
            gl::ARRAY_BUFFER,
            data_size as GLsizeiptr,
            std::mem::transmute(&FULLSCREEN_RECT[0]),
            gl::STATIC_DRAW,
        );

        let vert_count = FULLSCREEN_RECT.len() as GLsizei / 2;
        gl::DrawArrays(gl::TRIANGLES, 0, vert_count);
    }
}

pub fn compile_shader(src: &str, ty: GLenum) -> Result<GLuint, String> {
    unsafe {
        let shader = gl::CreateShader(ty);

        // Attempt to compile the shader
        let c_str = CString::new(src.as_bytes()).unwrap();
        gl::ShaderSource(shader, 1, &c_str.as_ptr(), std::ptr::null());
        gl::CompileShader(shader);

        // Get the compile status
        let mut status = gl::FALSE as GLint;
        gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut status);

        // Fail on error
        if status != (gl::TRUE as GLint) {
            let mut len = 0;
            gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut len);

            let mut buf = Vec::with_capacity(len as usize);
            buf.set_len((len as usize).saturating_sub(1));

            gl::GetShaderInfoLog(
                shader,
                len,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut GLchar,
            );

            let msg = std::str::from_utf8_unchecked(&buf);
            return Err(msg.into());
        }

        Ok(shader)
    }
}

/// Creates a program from a slice of shaders.
///
/// Creates a new program and attaches the given shaders to that program.
pub fn link_program(sh: &[GLuint]) -> Result<GLuint, String> {
    unsafe {
        let program = gl::CreateProgram();

        // Link program
        sh.iter().for_each(|&s| gl::AttachShader(program, s));
        gl::LinkProgram(program);

        // Get the link status
        let mut status = gl::FALSE as GLint;
        gl::GetProgramiv(program, gl::LINK_STATUS, &mut status);

        // Fail on error
        if status != (gl::TRUE as GLint) {
            let mut len: GLint = 0;
            gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut len);

            let mut buf = Vec::with_capacity(len as usize);
            buf.set_len((len as usize).saturating_sub(1));

            gl::GetProgramInfoLog(
                program,
                len,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut GLchar,
            );

            let msg = std::str::from_utf8_unchecked(&buf);
            return Err(msg.into());
        }

        Ok(program)
    }
}

#[allow(dead_code)]
pub fn test_compute_capabilities() {
    unsafe {
        let mut work_group_count_x = 0;
        let mut work_group_count_y = 0;
        let mut work_group_count_z = 0;
        gl::GetIntegeri_v(gl::MAX_COMPUTE_WORK_GROUP_COUNT, 0, &mut work_group_count_x);
        gl::GetIntegeri_v(gl::MAX_COMPUTE_WORK_GROUP_COUNT, 1, &mut work_group_count_y);
        gl::GetIntegeri_v(gl::MAX_COMPUTE_WORK_GROUP_COUNT, 2, &mut work_group_count_z);

        println!(
            "Work group count: {:?}, {:?}, {:?}",
            work_group_count_x, work_group_count_y, work_group_count_z
        );
        gl::GetIntegeri_v(gl::MAX_COMPUTE_WORK_GROUP_SIZE, 0, &mut work_group_count_x);
        gl::GetIntegeri_v(gl::MAX_COMPUTE_WORK_GROUP_SIZE, 1, &mut work_group_count_y);
        gl::GetIntegeri_v(gl::MAX_COMPUTE_WORK_GROUP_SIZE, 2, &mut work_group_count_z);
        println!(
            "Work group size: {:?}, {:?}, {:?}",
            work_group_count_x, work_group_count_y, work_group_count_z
        );

        let mut work_group_invocations = 0;
        gl::GetIntegerv(
            gl::MAX_COMPUTE_WORK_GROUP_INVOCATIONS,
            &mut work_group_invocations,
        );

        println!("Max work group invocations: {:?}", work_group_invocations);
    }
}

#[allow(dead_code)]
pub fn create_texture(width: GLsizei, height: GLsizei, index: GLuint) -> (GLuint, GLuint, GLuint) {
    unsafe {
        let mut tex = 0;
        let mut fb = 0;

        gl::GenTextures(1, &mut tex);
        gl::GenFramebuffers(1, &mut fb);

        gl::ActiveTexture(gl::TEXTURE0 + index);
        gl::BindTexture(gl::TEXTURE_2D, tex);
        gl::BindFramebuffer(gl::FRAMEBUFFER, fb);

        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);

        gl::TexParameteri(
            gl::TEXTURE_2D,
            gl::TEXTURE_MIN_FILTER,
            gl::LINEAR_MIPMAP_LINEAR as i32,
        );
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);

        gl::TexImage2D(
            gl::TEXTURE_2D,
            0,
            gl::RGBA as _,
            width,
            height,
            0,
            gl::RGBA as _,
            gl::FLOAT,
            std::ptr::null(),
        );

        gl::GenerateMipmap(gl::TEXTURE_2D);

        gl::FramebufferTexture2D(
            gl::FRAMEBUFFER,
            gl::COLOR_ATTACHMENT0,
            gl::TEXTURE_2D,
            tex,
            0,
        );

        assert_eq!(
            gl::CheckFramebufferStatus(gl::FRAMEBUFFER),
            gl::FRAMEBUFFER_COMPLETE
        );

        (tex, fb, index)
    }
}

use std::collections::HashMap;

use gl::types::*;

use crate::*;

use super::GeometryAttribute;

/// A struct represents a geometry.
#[derive(std::fmt::Debug)]
pub struct Geometry {
    /// Count of vertices.
    pub count: GLsizei,

    /// Drawing mode e.g. `gl::TRIANGLES` .
    pub mode: GLenum,

    /// Attributes of the geometry. Keys are attribute location.
    pub attributes: HashMap<GLuint, GeometryAttribute<GLfloat>>,

    /// Index buffer of the geometry.
    pub indices: Option<GeometryAttribute<GLuint>>,

    /// A vao object for this geometry.
    vao: Option<GLuint>,
}

impl Geometry {
    pub const ATTRIBUTE_POSITION: GLuint = 0;
    pub const ATTRIBUTE_NORMAL: GLuint = 1;
    pub const ATTRIBUTE_TEXCOORD0: GLuint = 2;

    pub fn init(count: GLsizei) -> Self {
        Geometry {
            count,
            mode: gl::TRIANGLES,
            attributes: HashMap::new(),
            indices: None,
            vao: None,
        }
    }

    /// Make a fullscreen rect geometry.
    pub fn fullscreen_rect() -> Self {
        let mut geometry = Geometry::init(4);
        geometry.mode = gl::TRIANGLE_STRIP;

        let attr_pos = GeometryAttribute::init(
            vec![-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0],
            2,
            gl::FLOAT,
        );
        geometry.attributes.insert(0, attr_pos);

        geometry
    }

    /// Make a vertex array object out of this geometry and assign it to its vao field.
    pub fn vao(&mut self) -> GLuint {
        match self.vao {
            None => {
                // vao
                let mut vao = 0;

                unsafe {
                    gl::GenVertexArrays(1, &mut vao);
                    gl::BindVertexArray(vao);
                    gl_debug_check!();
                }

                // indices
                if let Some(indices) = &mut self.indices {
                    indices.buffer();
                }

                // attributes
                for (index, attribute) in self.attributes.iter_mut() {
                    attribute.buffer();

                    unsafe {
                        gl::EnableVertexAttribArray(*index);
                        gl_debug_check!();
                    }

                    attribute.vertex_attrib_pointer(*index);
                }

                self.vao = Some(vao);

                vao
            }
            Some(vao) => vao,
        }
    }

    /// Delete the vertex array object.
    pub fn delete_vao(&mut self) {
        match self.vao {
            None => (),
            Some(vao) => {
                unsafe {
                    gl::DeleteVertexArrays(1, &vao);
                    // gl_debug_check!();
                }

                self.vao = None;
            }
        }
    }
}

impl Drop for Geometry {
    fn drop(&mut self) {
        self.delete_vao();
    }
}

#version 330
precision mediump float;
uniform mat4 projection;

in vec2 position;
in vec2 tex_coords;

out vec2 v_tex_coords;

uniform vec2 source_texture_dimensions;

uniform vec2 source_position;
uniform vec2 source_dimensions;

void main() {
    v_tex_coords = vec2(source_position + source_dimensions * tex_coords) / source_texture_dimensions;
    gl_Position = projection * vec4(position, 0.0, 1.0);
}
#version 330
in vec2 position;
in vec2 adjust;
in vec2 tex;
in vec4 fg_color;
in vec4 bg_color;
in float has_color;
in float underline;
in float v_idx;
uniform mat4 projection;
uniform mat4 translation;
uniform bool bg_and_line_layer;
out vec2 tex_coords;
out vec2 underline_coords;
out vec4 o_fg_color;
out vec4 o_bg_color;
out float o_has_color;
out float o_underline;
// Offset from the RHS texture coordinate to the LHS.
// This is an underestimation to avoid the shader interpolating
// the underline gylph into its neighbor.
const float underline_offset = (1.0 / 5.0);
void main() {
    o_fg_color = fg_color;
    o_bg_color = bg_color;
    o_has_color = has_color;
    o_underline = underline;
    if (bg_and_line_layer) {
        gl_Position = projection * vec4(position, 0.0, 1.0);
        if (underline != 0.0) {
            // Populate the underline texture coordinates based on the
            // v_idx (which tells us which corner of the cell we're
            // looking at) and o_underline which corresponds to one
            // of the U_XXX constants defined in the rust code below
            // and which holds the RHS position in the texture coordinate
            // space for the underline texture layer.
            if (v_idx == 0.0) { // top left
                underline_coords = vec2(o_underline - underline_offset, -1.0);
            } else if (v_idx == 1.0) { // top right
                underline_coords = vec2(o_underline, -1.0);
            } else if (v_idx == 2.0) { // bot left
                underline_coords = vec2(o_underline- underline_offset, 0.0);
            } else { // bot right
                underline_coords = vec2(o_underline, 0.0);
            }
        }
    } else {
        gl_Position = projection * vec4(position + adjust, 0.0, 1.0);
        tex_coords = tex;
    }
}
#version 330
precision mediump float;
in vec2 tex_coords;
in vec2 underline_coords;
in vec4 o_fg_color;
in vec4 o_bg_color;
in float o_has_color;
in float o_underline;

out vec4 color;
uniform sampler2D glyph_tex;
uniform sampler2D underline_tex;
uniform bool bg_and_line_layer;

float multiply_one(float src, float dst, float inv_dst_alpha, float inv_src_alpha) {
    return (src * dst) + (src * (inv_dst_alpha)) + (dst * (inv_src_alpha));
}

// Alpha-regulated multiply to colorize the glyph bitmap.
// The texture data is pre-multiplied by the alpha, so we need to divide
// by the alpha after multiplying to avoid having the colors be too dark.
vec4 multiply(vec4 src, vec4 dst) {
    float inv_src_alpha = 1.0 - src.a;
    float inv_dst_alpha = 1.0 - dst.a;

    return vec4(
        multiply_one(src.r, dst.r, inv_dst_alpha, inv_src_alpha) / dst.a,
        multiply_one(src.g, dst.g, inv_dst_alpha, inv_src_alpha) / dst.a,
        multiply_one(src.b, dst.b, inv_dst_alpha, inv_src_alpha) / dst.a,
        dst.a);
}

void main() {
    if (bg_and_line_layer) {
        color = o_bg_color;
        // If there's an underline/strike glyph, extract the pixel color
        // from the texture.  If the alpha value is non-zero then we'll
        // take that pixel, otherwise we'll use the background color.
        if (o_underline != 0.0) {
            // Compute the pixel color for this location
            vec4 under_color = multiply(o_fg_color, texture2D(underline_tex, underline_coords));
            if (under_color.a != 0.0) {
                // if the line glyph isn't transparent in this position then
                // we take this pixel color, otherwise we'll leave the color
                // at the background color.
                color = under_color;
            }
        }
    } else {
        color = texture2D(glyph_tex, tex_coords);
        if (o_has_color == 0.0) {
            // if it's not a color emoji, tint with the fg_color
            color = multiply(o_fg_color, color);
        }
    }
}
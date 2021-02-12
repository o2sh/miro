#version 300 es
precision mediump float;
in vec2 tex_coords;
in vec4 o_fg_color;
in vec4 o_bg_color;
in float o_has_color;
in float o_underline;

out vec4 color;
uniform sampler2D glyph_tex;
uniform sampler2D underline_tex;
uniform bool bg_fill;
uniform bool underlining;

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
    if (bg_fill) {
        color = o_bg_color;
    } else if (underlining) {
        if (o_underline != 0.0) {
            color = texture2D(underline_tex, tex_coords) * o_fg_color;
        } else {
            discard;
        }
    } else {
        color = texture2D(glyph_tex, tex_coords);
        if (o_has_color == 0.0) {
            // if it's not a color emoji, tint with the fg_color
            color = multiply(o_fg_color, color);
        }
    }
}
precision mediump float;

in vec2 o_tex;
in vec4 o_fg_color;
in vec4 o_bg_color;
in float o_has_color;
in vec2 o_underline;
in vec2 o_cursor;
in vec4 o_cursor_color;

uniform mat4 projection;
uniform bool bg_and_line_layer;
uniform sampler2D glyph_tex;

out vec4 color;

float multiply_one(float src, float dst, float inv_dst_alpha, float inv_src_alpha) {
  return (src * dst) + (src * (inv_dst_alpha)) + (dst * (inv_src_alpha));
}

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

    vec4 under_color = multiply(o_fg_color, texture(glyph_tex, o_underline));
    if (under_color.a != 0.0) {
        color = under_color;
    }

    vec4 cursor_outline = multiply(o_cursor_color, texture(glyph_tex, o_cursor));
    if (cursor_outline.a != 0.0) {
      color = cursor_outline;
    }

  } else {
    color = texture(glyph_tex, o_tex);
    if (o_has_color == 0.0) {
      color.rgb = o_fg_color.rgb;
    }
  }
}
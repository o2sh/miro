#version 300 es
precision mediump float;
uniform mat4 projection;
in vec2 position;
in vec3 color;
out vec3 vColor;
void main() {
    gl_Position = vec4(position, 0.0, 1.0) * projection;
    vColor = color;
}
#version 330 core
in vec3 inPosition;
in vec2 inUV;

uniform mat4 mat;

out float vHeight;
out vec2 vUV;

void main() {
    vHeight = inPosition.y;
    vUV = inUV;
    gl_Position = mat * vec4(inPosition, 1.0);
}

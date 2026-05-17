#version 330 core
in float vHeight;
in vec2 vUV;

out vec4 FragColor;

void main() {
    float t = vUV.x;

    vec3 low  = vec3(0.05, 0.0, 0.15);  // purple
    vec3 mid  = vec3(0.9, 0.1, 0.5);    // pink
    vec3 high = vec3(0.0, 0.9, 0.9);    // cyan

    vec3 color;
    if (t < 0.5) {
        color = mix(low, mid, t * 2.0);
    } else {
        color = mix(mid, high, (t - 0.5) * 2.0);
    }

    FragColor = vec4(color, 1.0);
}

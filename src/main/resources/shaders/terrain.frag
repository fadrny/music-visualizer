#version 330 core
in float vHeight;
in vec2 vUV;
in float vRadius;

out vec4 FragColor;

void main() {
    float radius = vRadius;
    float angle = vUV.y;

    // center hole
    float centerHole = smoothstep(0.03, 0.10, radius);

    // wider dynamic range
    float h = clamp(vHeight * 0.35, 0.0, 1.0);

    // 4-stop color mapping
    vec3 c0 = vec3(0.02, 0.0, 0.06);   // black
    vec3 c1 = vec3(0.15, 0.02, 0.35);  // indigo
    vec3 c2 = vec3(0.85, 0.05, 0.55);  // magenta
    vec3 c3 = vec3(0.1, 0.95, 0.95);   // cyan
    vec3 c4 = vec3(1.0, 1.0, 1.0);     // white

    vec3 color;
    if (h < 0.15) {
        color = mix(c0, c1, h / 0.15);
    } else if (h < 0.35) {
        color = mix(c1, c2, (h - 0.15) / 0.2);
    } else if (h < 0.7) {
        color = mix(c2, c3, (h - 0.35) / 0.35);
    } else {
        color = mix(c3, c4, (h - 0.7) / 0.3);
    }

    // vinyl grooves: thin rings
    float grooveFreq = radius * 200.0;
    float groove = 0.88 + 0.12 * pow(abs(sin(grooveFreq)), 8.0);
    color *= groove;

    // rim highlight at disc edge
    float rim = smoothstep(0.88, 0.98, radius) * (1.0 - smoothstep(0.98, 1.0, radius));
    color += rim * vec3(0.3, 0.1, 0.5) * (0.5 + h);

    // apply center hole
    color *= centerHole;

    // edge fade
    float edgeFade = 1.0 - smoothstep(0.95, 1.0, radius) * 0.6;
    color *= edgeFade;

    FragColor = vec4(color, 1.0);
}

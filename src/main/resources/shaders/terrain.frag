#version 330 core
in float vHeight;
in vec2 vUV;
in float vRadius;
in vec2 vLocalXZ;

out vec4 FragColor;

uniform sampler2D uAlbumArt;
uniform int uHasAlbumArt;
uniform float uMaxRadius;

void main() {
    float radius = vRadius;
    float angle = vUV.y;

    // Album art label
    float labelRadius = 0.20;
    float labelEdge = 0.015; // smooth transition width

    if (uHasAlbumArt == 1 && radius < labelRadius + labelEdge) {
        // Compute UV from local-space position
        float effectiveRadius = uMaxRadius * labelRadius;
        vec2 uv = vLocalXZ / (effectiveRadius * 2.0) + 0.5;

        // Circular mask
        float dist = length(vLocalXZ) / effectiveRadius;
        float circleMask = 1.0 - smoothstep(0.95, 1.0, dist);

        // Center hole
        float holeMask = smoothstep(0.005, 0.05, radius);

        // Sample album art
        vec3 artColor = texture(uAlbumArt, uv).rgb;

        // Blend factor: full art inside label, smooth transition to vinyl at edge
        float labelBlend = (1.0 - smoothstep(labelRadius - labelEdge, labelRadius + labelEdge, radius))
                           * circleMask * holeMask;

        if (labelBlend > 0.01) {
            // Vinyl color underneath
            float h = clamp(vHeight * 0.35, 0.0, 1.0);
            vec3 c0 = vec3(0.02, 0.0, 0.06);
            vec3 c1 = vec3(0.15, 0.02, 0.35);
            vec3 vinylColor = mix(c0, c1, h / 0.15);

            // 50% transparent art over vinyl, with smooth edge
            vec3 finalColor = mix(vinylColor, artColor, labelBlend * 0.2);
            FragColor = vec4(finalColor, 1.0);
            return;
        }
    }

    // center hole
    float centerHole = smoothstep(0.015, 0.04, radius);

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

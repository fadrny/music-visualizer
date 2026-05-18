#version 330 core
in vec3 inPosition;
in vec2 inUV;

uniform mat4 mat;
uniform float uFFT[64];
uniform float uAmplitude;
uniform float uTime;

out float vHeight;
out vec2 vUV;
out float vRadius;

void main() {
    float radius = inUV.x;
    float angle  = inUV.y;

    // FFT mapping with seamless wrap at the seam
    float binF = angle * 64.0;
    int bin0 = int(floor(binF)) % 64;
    int bin1 = (bin0 + 1) % 64;
    float frac = binF - floor(binF);

    // bin 0 wraps to bin 63
    float val0 = (bin0 < 64) ? uFFT[min(bin0, 63)] : uFFT[0];
    float val1 = (bin1 < 64) ? uFFT[min(bin1, 63)] : uFFT[0];
    float fftVal = mix(val0, val1, frac);

    // radial displacement curve
    float radialFalloff = smoothstep(0.08, 0.5, radius);

    // subtle ambient wave (disc breathes even in silence)
    float ambient = sin(radius * 12.0 - uTime * 2.0) * 0.02 * radius;

    float y = fftVal * uAmplitude * radialFalloff + ambient;

    // rotate the whole disc around Y axis
    float rotAngle = uTime * 0.25;
    float cosR = cos(rotAngle);
    float sinR = sin(rotAngle);
    float rx = inPosition.x * cosR - inPosition.z * sinR;
    float rz = inPosition.x * sinR + inPosition.z * cosR;

    vHeight = y;
    vUV = vec2(radius, angle);
    vRadius = radius;
    gl_Position = mat * vec4(rx, y, rz, 1.0);
}

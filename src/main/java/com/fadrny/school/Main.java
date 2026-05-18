package com.fadrny.school;

import org.lwjgl.BufferUtils;
import org.lwjgl.glfw.*;
import org.lwjgl.opengl.*;
import org.lwjgl.system.*;

import lwjglutils.OGLBuffers;
import lwjglutils.OGLTextRenderer;
import lwjglutils.OGLUtils;
import lwjglutils.ShaderUtils;
import lwjglutils.ToFloatArray;

import transforms.Camera;
import transforms.Mat4;
import transforms.Mat4PerspRH;
import transforms.Vec3D;

import java.nio.DoubleBuffer;
import java.nio.IntBuffer;

import static org.lwjgl.glfw.Callbacks.*;
import static org.lwjgl.glfw.GLFW.*;
import static org.lwjgl.opengl.GL11.*;
import static org.lwjgl.opengl.GL33.*;
import static org.lwjgl.system.MemoryStack.*;
import static org.lwjgl.system.MemoryUtil.*;

/**
 * Interactive Audio-Reactive Topography
 *
 * 3D disc deformed by audio FFT data in real-time.
 *
 * @author Marek Fadrný
 */
public class Main {

    // Window
    int width = 1280, height = 720;
    private long window;

    // Mouse
    double ox, oy;
    boolean mouseButton1 = false;

    // OpenGL objects
    OGLBuffers discBuffers;
    OGLTextRenderer textRenderer;
    int shaderProgram;
    int locMat, locFFT, locAmplitude, locTime;

    // Camera + Projection
    Camera cam = new Camera();
    Mat4 proj = new Mat4PerspRH(Math.PI / 4, 1, 0.01, 1000.0);

    // Audio
    AudioFFT audioFFT;

    // State
    boolean wireframe = false;
    float amplitude = 1.5f;

    private void init() {
        GLFWErrorCallback.createPrint(System.err).set();

        if (!glfwInit())
            throw new IllegalStateException("Unable to initialize GLFW");

        glfwDefaultWindowHints();
        glfwWindowHint(GLFW_VISIBLE, GLFW_FALSE);
        glfwWindowHint(GLFW_RESIZABLE, GLFW_TRUE);

        window = glfwCreateWindow(width, height, "Vinyl music visualizer", NULL, NULL);
        if (window == NULL)
            throw new RuntimeException("Failed to create the GLFW window");

        // Keyboard
        glfwSetKeyCallback(window, (window, key, scancode, action, mods) -> {
            if (key == GLFW_KEY_ESCAPE && action == GLFW_RELEASE)
                glfwSetWindowShouldClose(window, true);

            if (action == GLFW_PRESS || action == GLFW_REPEAT) {
                switch (key) {
                    case GLFW_KEY_W: cam = cam.forward(.2); break;
                    case GLFW_KEY_S: cam = cam.backward(.2); break;
                    case GLFW_KEY_A: cam = cam.left(.2); break;
                    case GLFW_KEY_D: cam = cam.right(.2); break;
                    case GLFW_KEY_TAB: wireframe = !wireframe; break;
                    case GLFW_KEY_KP_ADD:
                    case GLFW_KEY_EQUAL: amplitude = Math.min(amplitude + 0.1f, 5f); break;
                    case GLFW_KEY_KP_SUBTRACT:
                    case GLFW_KEY_MINUS: amplitude = Math.max(amplitude - 0.1f, 0f); break;
                    case GLFW_KEY_M: audioFFT.nextDevice(); break;
                }
            }
        });

        // Mouse look
        glfwSetCursorPosCallback(window, new GLFWCursorPosCallback() {
            @Override
            public void invoke(long window, double x, double y) {
                if (mouseButton1) {
                    cam = cam.addAzimuth(Math.PI * (ox - x) / width)
                             .addZenith(Math.PI * (oy - y) / width);
                    ox = x;
                    oy = y;
                }
            }
        });

        glfwSetMouseButtonCallback(window, new GLFWMouseButtonCallback() {
            @Override
            public void invoke(long window, int button, int action, int mods) {
                if (button == GLFW_MOUSE_BUTTON_1 && action == GLFW_PRESS) {
                    mouseButton1 = true;
                    DoubleBuffer xBuf = BufferUtils.createDoubleBuffer(1);
                    DoubleBuffer yBuf = BufferUtils.createDoubleBuffer(1);
                    glfwGetCursorPos(window, xBuf, yBuf);
                    ox = xBuf.get(0);
                    oy = yBuf.get(0);
                }
                if (button == GLFW_MOUSE_BUTTON_1 && action == GLFW_RELEASE) {
                    mouseButton1 = false;
                    DoubleBuffer xBuf = BufferUtils.createDoubleBuffer(1);
                    DoubleBuffer yBuf = BufferUtils.createDoubleBuffer(1);
                    glfwGetCursorPos(window, xBuf, yBuf);
                    double x = xBuf.get(0);
                    double y = yBuf.get(0);
                    cam = cam.addAzimuth(Math.PI * (ox - x) / width)
                             .addZenith(Math.PI * (oy - y) / width);
                    ox = x;
                    oy = y;
                }
            }
        });

        // Resize
        glfwSetFramebufferSizeCallback(window, new GLFWFramebufferSizeCallback() {
            @Override
            public void invoke(long window, int w, int h) {
                if (w > 0 && h > 0 && (w != width || h != height)) {
                    width = w;
                    height = h;
                    proj = new Mat4PerspRH(Math.PI / 4, height / (double) width, 0.01, 1000.0);
                    if (textRenderer != null)
                        textRenderer.resize(width, height);
                }
            }
        });

        // Center window
        try (MemoryStack stack = stackPush()) {
            IntBuffer pWidth = stack.mallocInt(1);
            IntBuffer pHeight = stack.mallocInt(1);
            glfwGetWindowSize(window, pWidth, pHeight);
            GLFWVidMode vidmode = glfwGetVideoMode(glfwGetPrimaryMonitor());
            glfwSetWindowPos(window,
                    (vidmode.width() - pWidth.get(0)) / 2,
                    (vidmode.height() - pHeight.get(0)) / 2);
        }

        glfwMakeContextCurrent(window);
        glfwSwapInterval(1); // V-Sync
        glfwShowWindow(window);

        GL.createCapabilities();
        OGLUtils.printOGLparameters();

        // Background
        glClearColor(0.01f, 0.0f, 0.03f, 1.0f);

        // Disc geometry
        createDiscBuffers();

        // Shaders
        shaderProgram = ShaderUtils.loadProgram("/shaders/terrain");
        glUseProgram(shaderProgram);
        locMat = glGetUniformLocation(shaderProgram, "mat");
        locFFT = glGetUniformLocation(shaderProgram, "uFFT");
        locAmplitude = glGetUniformLocation(shaderProgram, "uAmplitude");
        locTime = glGetUniformLocation(shaderProgram, "uTime");

        // Camera
        cam = cam.withPosition(new Vec3D(4, 6, 3))
                 .withAzimuth(Math.PI * 1.25)
                 .withZenith(Math.PI * -0.20);

        // OpenGL state
        glDisable(GL_CULL_FACE);
        glEnable(GL_DEPTH_TEST);
        glEnable(GL_LINE_SMOOTH);

        // Projection (initial)
        proj = new Mat4PerspRH(Math.PI / 4, height / (double) width, 0.01, 1000.0);

        // Text renderer
        textRenderer = new OGLTextRenderer(width, height);

        // Audio FFT
        audioFFT = new AudioFFT();
        audioFFT.start();
    }

    void createDiscBuffers() {
        int rings = 128;
        int sectors = 256;
        float maxRadius = 5f;

        // polar disc: position (x, 0, z) + uv (radius, angle)
        float[] verts = new float[rings * sectors * 5];
        for (int r = 0; r < rings; r++) {
            float radius = (float) r / (rings - 1) * maxRadius;
            for (int s = 0; s < sectors; s++) {
                float angle = (float) s / sectors * (float) (2 * Math.PI);
                int i = (r * sectors + s) * 5;
                verts[i]     = radius * (float) Math.cos(angle);
                verts[i + 1] = 0f;
                verts[i + 2] = radius * (float) Math.sin(angle);
                verts[i + 3] = radius / maxRadius;          // u = radius
                verts[i + 4] = (float) s / sectors;         // v = angle
            }
        }

        // 2 triangles per quad, sectors wrap around
        int[] indices = new int[(rings - 1) * sectors * 6];
        int idx = 0;
        for (int r = 0; r < rings - 1; r++) {
            for (int s = 0; s < sectors; s++) {
                int nextS = (s + 1) % sectors;
                int cur      = r * sectors + s;
                int curNext  = r * sectors + nextS;
                int ring     = (r + 1) * sectors + s;
                int ringNext = (r + 1) * sectors + nextS;

                indices[idx++] = cur;
                indices[idx++] = curNext;
                indices[idx++] = ring;
                indices[idx++] = curNext;
                indices[idx++] = ringNext;
                indices[idx++] = ring;
            }
        }

        OGLBuffers.Attrib[] attribs = {
                new OGLBuffers.Attrib("inPosition", 3),
                new OGLBuffers.Attrib("inUV", 2)
        };

        discBuffers = new OGLBuffers(verts, attribs, indices);
        System.out.println("Disc created: " + rings + " rings x " + sectors
                + " sectors (" + (indices.length / 3) + " triangles)");
    }

    private void loop() {
        while (!glfwWindowShouldClose(window)) {
            glViewport(0, 0, width, height);
            glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT);

            glUseProgram(shaderProgram);
            float time = (float) glfwGetTime();

            // MVP matrix
            glUniformMatrix4fv(locMat, false,
                    ToFloatArray.convert(cam.getViewMatrix().mul(proj)));

            // FFT data
            glUniform1fv(locFFT, audioFFT.getBins());
            glUniform1f(locAmplitude, amplitude);
            glUniform1f(locTime, time);

            // Wireframe
            if (wireframe) {
                glPolygonMode(GL_FRONT_AND_BACK, GL_LINE);
            } else {
                glPolygonMode(GL_FRONT_AND_BACK, GL_FILL);
            }

            // Draw disc
            discBuffers.draw(GL_TRIANGLES, shaderProgram);

            // HUD text
            String text = "WASD: move | Tab: wireframe ["
                    + (wireframe ? "ON" : "OFF") + "] | +/-: amp ["
                    + String.format("%.1f", amplitude) + "] | M: audio";
            textRenderer.addStr2D(3, 20, text);
            textRenderer.addStr2D(3, 35, "Audio: " + audioFFT.getDeviceName());
            textRenderer.addStr2D(width - 180, height - 3, "Marek Fadrny | PGRF2 2025/26");

            glfwSwapBuffers(window);
            glfwPollEvents();
        }
    }

    public void run() {
        try {
            init();
            loop();
            glfwFreeCallbacks(window);
            glfwDestroyWindow(window);
        } catch (Throwable t) {
            t.printStackTrace();
        } finally {
            if (audioFFT != null) audioFFT.stop();
            glDeleteProgram(shaderProgram);
            glfwTerminate();
            glfwSetErrorCallback(null).free();
        }
    }

    public static void main(String[] args) {
        new Main().run();
    }
}
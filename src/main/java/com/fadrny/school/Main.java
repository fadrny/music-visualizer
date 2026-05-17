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
 * 3D grid deformed by audio FFT data in real-time.
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
    OGLBuffers gridBuffers;
    OGLTextRenderer textRenderer;
    int shaderProgram;
    int locMat;

    // Camera & Projection
    Camera cam = new Camera();
    Mat4 proj = new Mat4PerspRH(Math.PI / 4, 1, 0.01, 1000.0);

    // State
    boolean wireframe = false;

    private void init() {
        GLFWErrorCallback.createPrint(System.err).set();

        if (!glfwInit())
            throw new IllegalStateException("Unable to initialize GLFW");

        glfwDefaultWindowHints();
        glfwWindowHint(GLFW_VISIBLE, GLFW_FALSE);
        glfwWindowHint(GLFW_RESIZABLE, GLFW_TRUE);

        window = glfwCreateWindow(width, height, "Audio-Reactive Topography", NULL, NULL);
        if (window == NULL)
            throw new RuntimeException("Failed to create the GLFW window");

        // Keyboard
        glfwSetKeyCallback(window, (window, key, scancode, action, mods) -> {
            if (key == GLFW_KEY_ESCAPE && action == GLFW_RELEASE)
                glfwSetWindowShouldClose(window, true);

            if (action == GLFW_PRESS || action == GLFW_REPEAT) {
                switch (key) {
                    case GLFW_KEY_W: cam = cam.forward(1); break;
                    case GLFW_KEY_S: cam = cam.backward(1); break;
                    case GLFW_KEY_A: cam = cam.left(1); break;
                    case GLFW_KEY_D: cam = cam.right(1); break;
                    case GLFW_KEY_LEFT_SHIFT: cam = cam.up(1); break;
                    case GLFW_KEY_LEFT_CONTROL: cam = cam.down(1); break;
                    case GLFW_KEY_SPACE:
                        cam = cam.withFirstPerson(!cam.getFirstPerson());
                        break;
                    case GLFW_KEY_R: cam = cam.mulRadius(0.9f); break;
                    case GLFW_KEY_F: cam = cam.mulRadius(1.1f); break;
                    case GLFW_KEY_TAB: wireframe = !wireframe; break;
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
        glClearColor(0.02f, 0.0f, 0.05f, 1.0f); // very dark purple

        // Grid geometry
        createGridBuffers();

        // Shaders
        shaderProgram = ShaderUtils.loadProgram("/shaders/terrain");
        glUseProgram(shaderProgram);
        locMat = glGetUniformLocation(shaderProgram, "mat");

        // Camera
        cam = cam.withPosition(new Vec3D(5, 5, 4))
                 .withAzimuth(Math.PI * 1.25)
                 .withZenith(Math.PI * -0.15);

        // OpenGL state
        glDisable(GL_CULL_FACE);
        glEnable(GL_DEPTH_TEST);

        // Projection (initial)
        proj = new Mat4PerspRH(Math.PI / 4, height / (double) width, 0.01, 1000.0);

        // Text renderer
        textRenderer = new OGLTextRenderer(width, height);
    }

    void createGridBuffers() {
        int rows = 64;
        int cols = 64;

        // Vertex data: position (x, y, z) + uv (u, v) = 5 floats per vertex
        float[] verts = new float[rows * cols * 5];
        for (int r = 0; r < rows; r++) {
            for (int c = 0; c < cols; c++) {
                int i = (r * cols + c) * 5;
                verts[i]     = (float) c / (cols - 1) * 10f - 5f;
                verts[i + 1] = 0f;
                verts[i + 2] = (float) r / (rows - 1) * 10f - 5f;
                verts[i + 3] = (float) c / (cols - 1);
                verts[i + 4] = (float) r / (rows - 1);
            }
        }

        // 2 triangles per cell = 6 indices per cell
        int[] indices = new int[(rows - 1) * (cols - 1) * 6];
        int idx = 0;
        for (int r = 0; r < rows - 1; r++) {
            for (int c = 0; c < cols - 1; c++) {
                int topLeft = r * cols + c;
                indices[idx++] = topLeft;
                indices[idx++] = topLeft + 1;
                indices[idx++] = topLeft + cols;
                indices[idx++] = topLeft + 1;
                indices[idx++] = topLeft + cols + 1;
                indices[idx++] = topLeft + cols;
            }
        }

        OGLBuffers.Attrib[] attribs = {
                new OGLBuffers.Attrib("inPosition", 3),
                new OGLBuffers.Attrib("inUV", 2)
        };

        gridBuffers = new OGLBuffers(verts, attribs, indices);
        System.out.println("Grid created: " + rows + "x" + cols
                + " (" + (indices.length / 3) + " triangles)");
    }

    private void loop() {
        while (!glfwWindowShouldClose(window)) {
            glViewport(0, 0, width, height);
            glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT);

            glUseProgram(shaderProgram);

            // MVP matrix
            glUniformMatrix4fv(locMat, false,
                    ToFloatArray.convert(cam.getViewMatrix().mul(proj)));

            // Wireframe
            if (wireframe) {
                glPolygonMode(GL_FRONT_AND_BACK, GL_LINE);
            } else {
                glPolygonMode(GL_FRONT_AND_BACK, GL_FILL);
            }

            // Draw grid
            gridBuffers.draw(GL_TRIANGLES, shaderProgram);

            // HUD text
            String text = "Audio-Reactive Topography | WASD: move | Mouse: look"
                    + " | Tab: wireframe [" + (wireframe ? "ON" : "OFF") + "]";
            textRenderer.addStr2D(3, 20, text);
            textRenderer.addStr2D(width - 220, height - 3, "Marek Fadrny | PGRF2 2025/26");

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
            glDeleteProgram(shaderProgram);
            glfwTerminate();
            glfwSetErrorCallback(null).free();
        }
    }

    public static void main(String[] args) {
        new Main().run();
    }
}
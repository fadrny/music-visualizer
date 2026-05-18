package com.fadrny.school;

import be.tarsos.dsp.AudioDispatcher;
import be.tarsos.dsp.AudioEvent;
import be.tarsos.dsp.AudioProcessor;
import be.tarsos.dsp.io.jvm.JVMAudioInputStream;
import be.tarsos.dsp.util.fft.FFT;

import javax.sound.sampled.*;
import java.util.ArrayList;
import java.util.List;

/**
 * FFT audio analysis using TarsosDSP.
 * Enumerates available audio input devices, allows runtime switching.
 * Separate thread, provides 64 frequency bins.
 *
 * @author Marek Fadrný
 */
public class AudioFFT {

    private static final int SAMPLE_RATE = 44100;
    private static final int BUFFER_SIZE = 2048;
    private static final int OVERLAP = 1024;
    private static final int BIN_COUNT = 64;

    private final float[] bins = new float[BIN_COUNT];
    private AudioDispatcher dispatcher;
    private final List<Mixer.Info> inputDevices = new ArrayList<>();
    private int currentDeviceIndex = 0;
    private String currentDeviceName = "none";

    public AudioFFT() {
        enumerateDevices();
    }

    private void enumerateDevices() {
        AudioFormat format = new AudioFormat(SAMPLE_RATE, 16, 1, true, false);
        DataLine.Info lineInfo = new DataLine.Info(TargetDataLine.class, format);

        inputDevices.clear();
        System.out.println("Available audio input devices:");

        Mixer.Info[] mixerInfos = AudioSystem.getMixerInfo();
        for (Mixer.Info info : mixerInfos) {
            Mixer mixer = AudioSystem.getMixer(info);
            if (mixer.isLineSupported(lineInfo)) {
                inputDevices.add(info);
                System.out.println("  [" + (inputDevices.size() - 1) + "] " + info.getName());
            }
        }
    }

    public void start() {
        if (!inputDevices.isEmpty()) {
            startDevice(0);
        } else {
            System.err.println("No audio input devices found");
        }
    }

    public void startDevice(int index) {
        stop();

        if (index < 0 || index >= inputDevices.size()) {
            System.err.println("Invalid device index: " + index);
            return;
        }

        currentDeviceIndex = index;
        Mixer.Info mixerInfo = inputDevices.get(index);
        currentDeviceName = mixerInfo.getName();

        try {
            AudioFormat format = new AudioFormat(SAMPLE_RATE, 16, 1, true, false);
            DataLine.Info lineInfo = new DataLine.Info(TargetDataLine.class, format);

            Mixer mixer = AudioSystem.getMixer(mixerInfo);
            TargetDataLine line = (TargetDataLine) mixer.getLine(lineInfo);
            line.open(format, BUFFER_SIZE * 2);
            line.start();

            AudioInputStream stream = new AudioInputStream(line);
            JVMAudioInputStream jvmStream = new JVMAudioInputStream(stream);
            dispatcher = new AudioDispatcher(jvmStream, BUFFER_SIZE, OVERLAP);

            dispatcher.addAudioProcessor(new AudioProcessor() {
                final FFT fft = new FFT(BUFFER_SIZE);
                final float[] amplitudes = new float[BUFFER_SIZE / 2];

                @Override
                public boolean process(AudioEvent audioEvent) {
                    float[] buffer = audioEvent.getFloatBuffer().clone();
                    fft.forwardTransform(buffer);
                    fft.modulus(buffer, amplitudes);

                    synchronized (bins) {
                        int specLen = amplitudes.length;
                        for (int i = 0; i < BIN_COUNT; i++) {
                            // logarithmic frequency mapping
                            double t0 = (double) i / BIN_COUNT;
                            double t1 = (double) (i + 1) / BIN_COUNT;
                            int from = (int) (Math.pow(t0, 2.0) * specLen);
                            int to   = (int) (Math.pow(t1, 2.0) * specLen);
                            if (to <= from) to = from + 1;
                            if (to > specLen) to = specLen;

                            float sum = 0;
                            for (int j = from; j < to; j++) {
                                sum += amplitudes[j];
                            }
                            float avg = sum / (to - from);

                            // log amplitude
                            float val = (float) (Math.log1p(avg * 50.0) / Math.log(51.0));

                            // smoothing
                            bins[i] = bins[i] * 0.85f + val * 0.15f;
                        }
                    }
                    return true;
                }

                @Override
                public void processingFinished() {}
            });

            Thread audioThread = new Thread(dispatcher, "Audio-FFT");
            audioThread.setDaemon(true);
            audioThread.start();
            System.out.println("Audio FFT started: " + currentDeviceName);

        } catch (LineUnavailableException e) {
            System.err.println("Cannot open device: " + e.getMessage());
        }
    }

    public void nextDevice() {
        if (inputDevices.isEmpty()) return;
        int next = (currentDeviceIndex + 1) % inputDevices.size();
        startDevice(next);
    }

    public float[] getBins() {
        synchronized (bins) {
            return bins.clone();
        }
    }

    public String getDeviceName() {
        return currentDeviceName;
    }

    public int getDeviceCount() {
        return inputDevices.size();
    }

    public void stop() {
        if (dispatcher != null) {
            dispatcher.stop();
            dispatcher = null;
        }
    }
}

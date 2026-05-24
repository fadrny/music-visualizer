use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rustfft::{num_complex::Complex, FftPlanner};
use std::sync::{Arc, Mutex};

const BUFFER_SIZE: usize = 2048;
const HOP: usize = 1024;
pub const BIN_COUNT: usize = 64;

pub struct AudioFFT {
    devices: Vec<cpal::Device>,
    current_idx: usize,
    current_name: String,
    bins: Arc<Mutex<[f32; BIN_COUNT]>>,
    stream: Option<cpal::Stream>,
}

impl AudioFFT {
    pub fn new() -> Self {
        let host = cpal::default_host();
        let mut devices: Vec<cpal::Device> = Vec::new();
        println!("Available audio input devices:");
        if let Ok(iter) = host.input_devices() {
            for (i, d) in iter.enumerate() {
                let name = d.name().unwrap_or_else(|_| "unknown".to_string());
                println!("  [{}] {}", i, name);
                devices.push(d);
            }
        }
        Self {
            devices,
            current_idx: 0,
            current_name: "none".to_string(),
            bins: Arc::new(Mutex::new([0.0; BIN_COUNT])),
            stream: None,
        }
    }

    pub fn start(&mut self) {
        if !self.devices.is_empty() {
            self.start_device(0);
        } else {
            eprintln!("No audio input devices found");
        }
    }

    pub fn next_device(&mut self) {
        if self.devices.is_empty() { return; }
        let next = (self.current_idx + 1) % self.devices.len();
        self.start_device(next);
    }

    pub fn device_name(&self) -> &str { &self.current_name }

    pub fn get_bins(&self) -> [f32; BIN_COUNT] {
        *self.bins.lock().unwrap()
    }

    pub fn stop(&mut self) { self.stream = None; }

    fn start_device(&mut self, index: usize) {
        self.stream = None;

        if index >= self.devices.len() {
            eprintln!("Invalid device index: {}", index);
            return;
        }
        self.current_idx = index;
        let device = &self.devices[index];
        self.current_name = device.name().unwrap_or_else(|_| "unknown".to_string());

        // Pick a config — try default input config, fall back to any supported.
        let config = match device.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Cannot open device {}: {}", self.current_name, e);
                return;
            }
        };
        let sample_format = config.sample_format();
        let channels = config.channels() as usize;
        let stream_config: cpal::StreamConfig = config.into();

        let bins = self.bins.clone();
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(BUFFER_SIZE);
        let mut ring: Vec<f32> = Vec::with_capacity(BUFFER_SIZE * 2);
        let mut last_hop: usize = 0;

        let err_fn = |e| eprintln!("audio stream error: {}", e);

        let mut process = move |samples: &[f32]| {
            // mix down to mono and append
            if channels <= 1 {
                ring.extend_from_slice(samples);
            } else {
                for frame in samples.chunks(channels) {
                    let mut s = 0.0;
                    for &c in frame { s += c; }
                    ring.push(s / channels as f32);
                }
            }
            // process while we have a full window
            while ring.len() - last_hop >= BUFFER_SIZE.saturating_sub(0) && ring.len() >= BUFFER_SIZE {
                // window of BUFFER_SIZE starting at 0
                let mut buf: Vec<Complex<f32>> = ring[..BUFFER_SIZE]
                    .iter()
                    .map(|&v| Complex { re: v, im: 0.0 })
                    .collect();
                fft.process(&mut buf);

                let half = BUFFER_SIZE / 2;
                let amps: Vec<f32> = buf[..half].iter().map(|c| (c.re * c.re + c.im * c.im).sqrt()).collect();

                let mut new_bins = [0.0f32; BIN_COUNT];
                {
                    let cur = bins.lock().unwrap();
                    new_bins.copy_from_slice(&cur[..]);
                }
                let spec_len = amps.len();
                for i in 0..BIN_COUNT {
                    let t0 = i as f64 / BIN_COUNT as f64;
                    let t1 = (i as f64 + 1.0) / BIN_COUNT as f64;
                    let mut from = (t0.powf(2.0) * spec_len as f64) as usize;
                    let mut to = (t1.powf(2.0) * spec_len as f64) as usize;
                    if to <= from { to = from + 1; }
                    if to > spec_len { to = spec_len; }
                    if from >= spec_len { from = spec_len - 1; to = spec_len; }
                    let mut sum = 0.0f32;
                    for j in from..to { sum += amps[j]; }
                    let avg = sum / (to - from) as f32;
                    let val = ((avg as f64 * 50.0).ln_1p() / 51.0_f64.ln()) as f32;
                    new_bins[i] = new_bins[i] * 0.85 + val * 0.15;
                }
                *bins.lock().unwrap() = new_bins;

                // advance by HOP (drop first HOP samples)
                ring.drain(..HOP);
                last_hop = 0;
            }
            // keep memory bounded
            if ring.len() > BUFFER_SIZE * 4 {
                let drop = ring.len() - BUFFER_SIZE * 2;
                ring.drain(..drop);
            }
        };

        let stream_result = match sample_format {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| process(data),
                err_fn,
                None,
            ),
            cpal::SampleFormat::I16 => {
                let mut buf = Vec::<f32>::new();
                device.build_input_stream(
                    &stream_config,
                    move |data: &[i16], _| {
                        buf.clear();
                        buf.extend(data.iter().map(|&s| s as f32 / i16::MAX as f32));
                        process(&buf);
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::U16 => {
                let mut buf = Vec::<f32>::new();
                device.build_input_stream(
                    &stream_config,
                    move |data: &[u16], _| {
                        buf.clear();
                        buf.extend(data.iter().map(|&s| (s as f32 - 32768.0) / 32768.0));
                        process(&buf);
                    },
                    err_fn,
                    None,
                )
            }
            other => {
                eprintln!("unsupported sample format: {:?}", other);
                return;
            }
        };

        match stream_result {
            Ok(s) => {
                if let Err(e) = s.play() {
                    eprintln!("stream play error: {}", e);
                    return;
                }
                self.stream = Some(s);
                println!("Audio FFT started: {}", self.current_name);
            }
            Err(e) => eprintln!("build_input_stream: {}", e),
        }
    }
}

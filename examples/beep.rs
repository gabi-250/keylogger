// A modified version of the `cpal` beeper example.
// See: https://github.com/RustAudio/cpal/blob/master/examples/beep.rs

use async_trait::async_trait;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use keylogger::{KeyEvent, KeyEventCause, KeyEventHandler, Keylogger, KeyloggerError};
use std::io;
use std::path::Path;

struct Beeper(cpal::Device);

#[async_trait]
impl KeyEventHandler for Beeper {
    async fn handle_events(&self, kb_device: &Path, kb_name: &str, events: Vec<KeyEvent>) {
        println!("[{} @ {}]: ev={:?}", kb_name, kb_device.display(), events);

        let config: cpal::SupportedStreamConfig = self.0.default_output_config().unwrap();

        for e in events {
            // Only handle key presses
            if e.cause == KeyEventCause::Release {
                continue;
            }

            match config.sample_format() {
                cpal::SampleFormat::F32 => {
                    run::<f32>(&self.0, &config.clone().into(), e.code as u16)
                }
                cpal::SampleFormat::I16 => {
                    run::<i16>(&self.0, &config.clone().into(), e.code as u16)
                }
                cpal::SampleFormat::U16 => {
                    run::<u16>(&self.0, &config.clone().into(), e.code as u16)
                }
            }
            .unwrap();
        }
    }
}

pub fn run<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    key_code: u16,
) -> Result<(), io::Error>
where
    T: cpal::Sample,
{
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;

    let mut sample_clock = 0f32;
    let mut next_value = move || {
        sample_clock = (sample_clock + 1.0) % sample_rate;
        (sample_clock * 440.0 * key_code as f32 * std::f32::consts::PI / sample_rate).sin()
    };

    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                write_data(data, channels, &mut next_value)
            },
            err_fn,
        )
        .unwrap();
    stream.play().unwrap();

    std::thread::sleep(std::time::Duration::from_millis(50));

    Ok(())
}

fn write_data<T>(output: &mut [T], channels: usize, next_sample: &mut dyn FnMut() -> f32)
where
    T: cpal::Sample,
{
    for frame in output.chunks_mut(channels) {
        let value: T = cpal::Sample::from::<f32>(&next_sample());
        for sample in frame.iter_mut() {
            *sample = value;
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), KeyloggerError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("failed to find output device");

    let beeper = Beeper(device);
    let keylogger = Keylogger::new(beeper)?;
    keylogger.capture().await?;

    Ok(())
}

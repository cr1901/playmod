use cpal;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use eyre::{ContextCompat, Result, eyre};
use modfile::ptmf::{self, SampleInfo};

use std::env;
use std::fs::File;
use std::io::BufReader;

fn main() -> eyre::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        return Err(eyre!("Mod filename required."));
    }

    let ref filename = args[1];
    let file = File::open(filename)?;

    let mut reader = BufReader::new(&file);
    let mut module = ptmf::read_mod(&mut reader, false).unwrap();

    let host = cpal::default_host();
    let device = host.default_output_device().wrap_err("failed to find output device")?;
    let config = device.default_output_config()?;

    match config.sample_format() {
        cpal::SampleFormat::F32 => run::<f32>(&device, &config.into(), module.sample_info),
        cpal::SampleFormat::I16 => run::<i16>(&device, &config.into(), module.sample_info),
        cpal::SampleFormat::U16 => run::<u16>(&device, &config.into(), module.sample_info),
    }
}

pub fn run<T>(device: &cpal::Device, config: &cpal::StreamConfig, samples: Vec<SampleInfo>) -> Result<()>
where
    T: cpal::Sample,
{
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;

    // Produce a sinusoid of maximum amplitude.
    let mut sample_clock = 0f32;
    let mut i = 0;
    let mut curr = 0;

    let mut next_value = move || {
        loop {
            let sample = &samples[curr];
            i += 1;
            if i >= sample.length {
                i = 0;
                curr += 1;
                continue;
            }
            sample_clock = (sample_clock + 1.0) % sample_rate;
            return sample.data[i as usize] as i16
        }

        // (1024.0*((sample_clock * 440.0 * 2.0 * std::f32::consts::PI / sample_rate).sin())) as i16
    };

    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            write_data(data, channels, &mut next_value)
        },
        err_fn,
    )?;
    stream.play()?;

    std::thread::sleep(std::time::Duration::from_millis(1000));

    Ok(())
}

fn write_data<T>(output: &mut [T], channels: usize, next_sample: &mut dyn FnMut() -> i16)
where
    T: cpal::Sample,
{
    for frame in output.chunks_mut(channels) {
        let value: T = cpal::Sample::from::<i16>(&next_sample());
        for sample in frame.iter_mut() {
            *sample = value;
        }
    }
}

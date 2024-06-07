use cpal;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use eyre::{ContextCompat, Result, eyre};
use modfile::ptmf::{self, SampleInfo};

use std::env;
use std::fs::File;
use std::io::BufReader;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;

static BUFFER: Lazy<Arc<Mutex<VecDeque<i16>>>> = Lazy::new(|| Arc::new(Mutex::new(VecDeque::new())));


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
        cpal::SampleFormat::U16 => unimplemented!()
        /* cpal::SampleFormat::U16 => run::<u16>(&device, &config.into(), module.sample_info), */
    }
}

pub fn run<T>(device: &cpal::Device, config: &cpal::StreamConfig, samples: Vec<SampleInfo>) -> Result<()>
where
    T: cpal::Sample + From<i16>,
{
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;

    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            write_data(data, channels)
        },
        err_fn,
    )?;
    stream.play()?;

    std::thread::spawn(move || {
        // Produce a sinusoid of maximum amplitude.
        let mut sample_clock = 0f32;
        let mut freq = 440.0;

        loop {
            let mut deque = BUFFER.lock().unwrap();

            if sample_clock == 48000.0*1.0 {
                freq = 880.0;
                deque.clear();
            }

            if deque.len() < 10000 {
                deque.push_back((1024.0*((sample_clock * freq * 2.0 * std::f32::consts::PI / sample_rate).sin())) as i16);
                sample_clock += 1.0;
            }
        }
    });

    std::thread::sleep(std::time::Duration::from_millis(2000));

    Ok(())
}

fn write_data<T>(output: &mut [T], channels: usize, /* next_sample: &mut dyn FnMut() -> i16 */)
where
    T: cpal::Sample + From<i16>
{
    let mut deque = BUFFER.lock().unwrap();
    let mut count = 0;
    let (buf0, buf1) = deque.as_slices();

    let mut sample_bufs = buf0.iter().chain(buf1);

    for frame in output.chunks_mut(channels) {
        let raw = match sample_bufs.next() {
            Some(i) => {
                count += 1;
                i
            },
            None => {
                // This is hopefully rare, but at least will prevent panic
                // from draining elements that weren't actually used.
                &0
            }
        };

        let value: T = cpal::Sample::from::<i16>(raw);
        for sample in frame.iter_mut() {
            *sample = value;
        }
    }

    // println!("{}", deque.capacity());
    deque.drain(0..count);
}

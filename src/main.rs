use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{self};
use eyre::{eyre, ContextCompat};
use modfile::ptmf::{self};

use std::env;
use std::fs::File;
use std::io::BufReader;

use playmod::*;

fn main() -> eyre::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        return Err(eyre!("Mod filename required."));
    }

    let ref filename = args[1];
    let file = File::open(filename)?;

    let mut reader = BufReader::new(&file);
    let module = ptmf::read_mod(&mut reader, false).unwrap();

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .wrap_err("failed to find output device")?;
    let config = device.default_output_config()?;
    let sample_rate = config.sample_rate().0;

    let sink = match config.sample_format() {
        cpal::SampleFormat::F32 => Sink::new::<f32>(&device, &config.into())?,
        cpal::SampleFormat::I16 => Sink::new::<i16>(&device, &config.into())?,
        cpal::SampleFormat::U16 => unimplemented!(), /* cpal::SampleFormat::U16 => run::<u16>(&device, &config.into(), module.sample_info), */
    };

    sink.start()?;

    let mut mixing_buf = vec![0i16; 960];
    play_mod(module, sink, &mut mixing_buf, sample_rate);

    Ok(())
}

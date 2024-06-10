use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{self, Stream};
use eyre::{eyre, ContextCompat, Result};
use modfile::ptmf::{self, Channel, SampleInfo};

use clap::{Parser, Subcommand};
use clap::ValueEnum;

use once_cell::sync::Lazy;
use std::collections::VecDeque;
use std::fs::File;
use std::io::BufReader;
use std::sync::{Arc, Mutex};

use playmod::*;


#[derive(clap::Parser)]
#[clap(author, version)]
/// Switch monitor input source from command-line
pub struct Args {
    pub modfile: String,
    #[clap(value_enum)]
    pub note: Note,
    pub sample_no: u8
}

fn main() -> eyre::Result<()> {
    let args = Args::parse();

    let file = File::open(args.modfile)?;

    let mut reader = BufReader::new(&file);
    let module = ptmf::read_mod(&mut reader, false).unwrap();

    if args.sample_no < 1 || args.sample_no > 31 {
        return Err(eyre!("sample number must be between 1 and 31"));
    }

    let sample = &module.sample_info[args.sample_no as usize - 1];

    if sample.length == 0 {
        return Err(eyre!("sample seems to be unimplemented"));
    }

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .wrap_err("failed to find output device").unwrap();
    let config = device.default_output_config().unwrap();
    let sample_rate = config.sample_rate().0;

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => run::<f32>(&device, &config.into())?,
        cpal::SampleFormat::I16 => run::<i16>(&device, &config.into())?,
        cpal::SampleFormat::U16 => unimplemented!(), /* cpal::SampleFormat::U16 => run::<u16>(&device, &config.into(), module.sample_info), */
    };

    stream.play()?;

    let mut state = SampleState {
        looped_yet: false,
        sample_offset: 0,
        sample_frac: 0
    };

    println!("{}", sample.length);
    for _ in 0..200 {
        play_tick(&mut state, sample, args.note, sample_rate);
    }

    Ok(())
}



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

static BUFFER: Lazy<Arc<Mutex<VecDeque<i16>>>> =
    Lazy::new(|| Arc::new(Mutex::new(VecDeque::new())));

#[derive(clap::Parser)]
#[clap(author, version)]
/// Switch monitor input source from command-line
pub struct Args {
    pub modfile: String,
    #[clap(value_enum)]
    pub note: Note,
    pub sample_no: u8
}

#[derive(Copy, Clone, ValueEnum)]
pub enum Note {
    #[clap(name = "C1")]
    C1 = 856,
    #[clap(name = "C#1")]
    CS1 = 808,
    #[clap(name = "D1")]
    D1 = 762,
    #[clap(name = "D#1")]
    DS1 = 720,
    #[clap(name = "E1")]
    E1 = 678,
    #[clap(name = "F1")]
    F1 = 640,
    #[clap(name = "F#1")]
    FS1 = 604,
    #[clap(name = "G1")]
    G1 = 570,
    #[clap(name = "G#1")]
    GS1 = 538,
    #[clap(name = "A1")]
    A1 = 508,
    #[clap(name = "A#1")]
    AS1 = 480,
    #[clap(name = "B1")]
    B1 = 453,
    #[clap(name = "C2")]
    C2 = 428,
    #[clap(name = "C#2")]
    CS2 = 404,
    #[clap(name = "D2")]
    D2 = 381,
    #[clap(name = "D#2")]
    DS2 = 360,
    #[clap(name = "E2")]
    E2 = 339,
    #[clap(name = "F2")]
    F2 = 320,
    #[clap(name = "F#2")]
    FS2 = 302,
    #[clap(name = "G2")]
    G2 = 285,
    #[clap(name = "G#2")]
    GS2 = 269,
    #[clap(name = "A2")]
    A2 = 254,
    #[clap(name = "A#2")]
    AS2 = 240,
    #[clap(name = "B2")]
    B2 = 226,
    #[clap(name = "C3")]
    C3 = 214,
    #[clap(name = "C#3")]
    CS3 = 202,
    #[clap(name = "D3")]
    D3 = 190,
    #[clap(name = "D#3")]
    DS3 = 180,
    #[clap(name = "E3")]
    E3 = 170,
    #[clap(name = "F3")]
    F3 = 160,
    #[clap(name = "F#3")]
    FS3 = 151,
    #[clap(name = "G3")]
    G3 = 143,
    #[clap(name = "G#3")]
    GS3 = 135,
    #[clap(name = "A3")]
    A3 = 127,
    #[clap(name = "A#3")]
    AS3 = 120,
    #[clap(name = "B3")]
    B3 = 113
}

pub struct SampleState {
    pub looped_yet: bool,
    pub sample_offset: u16,
    pub sample_frac: u8
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


pub fn play_tick(state: &mut SampleState, sample: &SampleInfo, period: Note, sample_rate: u32) {
    // FIXME: Get rid of floating point, I don't want it... used fixed-point
    // increments if we have to.
    let freq = 7093789.2 / (period as u16 as f32 * 2.0);
    let sample_rate = sample_rate as f32;

    let inc_rate = (((freq /sample_rate) * 256.0) as u32 >> 8) as u16;
    let inc_rate_frac: u8 = (((freq / sample_rate) * 256.0) as u32 % 256) as u8;

    let host_samples_per_tick = (sample_rate / 50.0) as u16;

    println!("{}, {}, {}, {}", freq, sample_rate, inc_rate, inc_rate_frac);

    'wait: loop {
        let mut deque = BUFFER.lock().unwrap();
        if deque.len() > 1000 {
            continue 'wait;
        }

        for _ in 0..host_samples_per_tick {
            let (new_frac, carry) = state.sample_frac.overflowing_add(inc_rate_frac);
            state.sample_frac = new_frac;

            if carry {
                state.sample_offset += inc_rate + 1;
            } else {
                state.sample_offset += inc_rate;
            }

            if !state.looped_yet || sample.repeat_length <= 2 {
                if state.sample_offset >= sample.length*2 {
                    state.looped_yet = true;
                    state.sample_offset = sample.repeat_start*2 + (state.sample_offset - sample.length*2);
                }
            } else {
                if state.sample_offset >= sample.repeat_start*2 + sample.repeat_length*2 {
                    state.sample_offset = state.sample_offset - (sample.repeat_start*2 + sample.repeat_length*2);
                }
            }

            let curr_sample_val = sample.data[state.sample_offset as usize] as i8 as i16;
            deque.push_back(curr_sample_val << 3)      
        }

        break 'wait;
    }
}

pub fn run<T>(device: &cpal::Device, config: &cpal::StreamConfig) -> Result<Stream>
where
    T: cpal::Sample + From<i16>,
{
    let channels = config.channels as usize;

    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            // // println!("In callback");
            write_data(data, channels)
        },
        err_fn,
    )?;
    Ok(stream)
}

fn write_data<T>(output: &mut [T], channels: usize /* next_sample: &mut dyn FnMut() -> i16 */)
where
    T: cpal::Sample + From<i16>,
{
    let mut deque = BUFFER.lock().unwrap();
    let mut count = 0;
    let (buf0, buf1) = deque.as_slices();

    // // println!("first {:?}, rest: {:?}", buf0, buf1);
    let mut sample_bufs = buf0.iter().chain(buf1);

    // // println!("Here {}", deque.len());
    for frame in output.chunks_mut(channels) {
        let raw = match sample_bufs.next() {
            Some(i) => {
                count += 1;
                i
            }
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

    // // println!("{}", deque.capacity());
    deque.drain(0..count);
}

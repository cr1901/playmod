use cpal::traits::DeviceTrait;
use cpal::{self, Stream};
use eyre::Result;
use modfile::ptmf::SampleInfo;

use clap::ValueEnum;

use once_cell::sync::Lazy;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

static BUFFER: Lazy<Arc<Mutex<VecDeque<i16>>>> =
    Lazy::new(|| Arc::new(Mutex::new(VecDeque::new())));

#[derive(Copy, Clone, ValueEnum)]
pub enum Note {
    #[cfg_attr(feature = "clap", clap(name = "C1"))]
    C1 = 856,
    #[cfg_attr(feature = "clap", clap(name = "C#1"))]
    CS1 = 808,
    #[cfg_attr(feature = "clap", clap(name = "D1"))]
    D1 = 762,
    #[cfg_attr(feature = "clap", clap(name = "D#1"))]
    DS1 = 720,
    #[cfg_attr(feature = "clap", clap(name = "E1"))]
    E1 = 678,
    #[cfg_attr(feature = "clap", clap(name = "F1"))]
    F1 = 640,
    #[cfg_attr(feature = "clap", clap(name = "F#1"))]
    FS1 = 604,
    #[cfg_attr(feature = "clap", clap(name = "G1"))]
    G1 = 570,
    #[cfg_attr(feature = "clap", clap(name = "G#1"))]
    GS1 = 538,
    #[cfg_attr(feature = "clap", clap(name = "A1"))]
    A1 = 508,
    #[cfg_attr(feature = "clap", clap(name = "A#1"))]
    AS1 = 480,
    #[cfg_attr(feature = "clap", clap(name = "B1"))]
    B1 = 453,
    #[cfg_attr(feature = "clap", clap(name = "C2"))]
    C2 = 428,
    #[cfg_attr(feature = "clap", clap(name = "C#2"))]
    CS2 = 404,
    #[cfg_attr(feature = "clap", clap(name = "D2"))]
    D2 = 381,
    #[cfg_attr(feature = "clap", clap(name = "D#2"))]
    DS2 = 360,
    #[cfg_attr(feature = "clap", clap(name = "E2"))]
    E2 = 339,
    #[cfg_attr(feature = "clap", clap(name = "F2"))]
    F2 = 320,
    #[cfg_attr(feature = "clap", clap(name = "F#2"))]
    FS2 = 302,
    #[cfg_attr(feature = "clap", clap(name = "G2"))]
    G2 = 285,
    #[cfg_attr(feature = "clap", clap(name = "G#2"))]
    GS2 = 269,
    #[cfg_attr(feature = "clap", clap(name = "A2"))]
    A2 = 254,
    #[cfg_attr(feature = "clap", clap(name = "A#2"))]
    AS2 = 240,
    #[cfg_attr(feature = "clap", clap(name = "B2"))]
    B2 = 226,
    #[cfg_attr(feature = "clap", clap(name = "C3"))]
    C3 = 214,
    #[cfg_attr(feature = "clap", clap(name = "C#3"))]
    CS3 = 202,
    #[cfg_attr(feature = "clap", clap(name = "D3"))]
    D3 = 190,
    #[cfg_attr(feature = "clap", clap(name = "D#3"))]
    DS3 = 180,
    #[cfg_attr(feature = "clap", clap(name = "E3"))]
    E3 = 170,
    #[cfg_attr(feature = "clap", clap(name = "F3"))]
    F3 = 160,
    #[cfg_attr(feature = "clap", clap(name = "F#3"))]
    FS3 = 151,
    #[cfg_attr(feature = "clap", clap(name = "G3"))]
    G3 = 143,
    #[cfg_attr(feature = "clap", clap(name = "G#3"))]
    GS3 = 135,
    #[cfg_attr(feature = "clap", clap(name = "A3"))]
    A3 = 127,
    #[cfg_attr(feature = "clap", clap(name = "A#3"))]
    AS3 = 120,
    #[cfg_attr(feature = "clap", clap(name = "B3"))]
    B3 = 113,
}

#[derive(Debug)]
pub struct SampleState {
    pub looped_yet: bool,
    pub sample_offset: u16,
    pub sample_frac: u8,
}

pub fn mix_sample_for_tick<P>(
    buf: &mut Vec<i16>,
    state: &mut SampleState,
    sample: &SampleInfo,
    period: P,
    sample_rate: u32,
) where
    P: Into<u16>,
{
    // FIXME: Get rid of floating point, I don't want it... used fixed-point
    // increments if we have to.
    // 7159090.5 for NTSC
    let freq = 7093789.2 / (period.into() as f32 * 2.0);
    let sample_rate = sample_rate as f32;

    let inc_rate = (((freq / sample_rate) * 256.0) as u32 >> 8) as u16;
    let inc_rate_frac: u8 = (((freq / sample_rate) * 256.0) as u32 % 256) as u8;

    // 60.0 for NTSC
    let host_samples_per_tick = (sample_rate / 50.0) as u16;
    buf.truncate(host_samples_per_tick as usize);

    for i in 0..host_samples_per_tick {
        if sample.repeat_length <= 2 && state.looped_yet {
            break;
        }

        let (new_frac, carry) = state.sample_frac.overflowing_add(inc_rate_frac);
        state.sample_frac = new_frac;

        if carry {
            state.sample_offset += inc_rate + 1;
        } else {
            // println!("{}, {}", state.sample_offset, inc_rate);
            state.sample_offset += inc_rate;
        }

        if sample.repeat_length <= 2 {
            if state.sample_offset >= sample.length * 2 {
                state.looped_yet = true;
                state.sample_offset =
                    sample.repeat_start * 2 + (state.sample_offset - sample.length * 2);
            }
        } else {
            if state.sample_offset >= sample.repeat_start * 2 + sample.repeat_length * 2 {
                state.sample_offset =
                    state.sample_offset - (sample.repeat_start * 2 + sample.repeat_length * 2);
            }
        }

        let curr_sample_val = sample.data[state.sample_offset as usize] as i8 as i16;
        buf[i as usize] += curr_sample_val << 3; // Raw values are a bit too quiet.
    }
}

pub fn dump_buf(buf: &Vec<i16>) {
    'wait: loop {
        let mut deque = BUFFER.lock().unwrap();
        if deque.len() > 1000 {
            drop(deque);
            // Don't busy loop/waste cycles.
            std::thread::sleep(Duration::from_millis(10));
            continue 'wait;
        }

        for b in buf {
            deque.push_back(*b);
        }

        break 'wait;
    }
}

pub fn play_tick(state: &mut SampleState, sample: &SampleInfo, period: Note, sample_rate: u32) {
    // FIXME: Get rid of floating point, I don't want it... used fixed-point
    // increments if we have to.
    let freq = 7093789.2 / (period as u16 as f32 * 2.0);
    let sample_rate = sample_rate as f32;

    let inc_rate = (((freq / sample_rate) * 256.0) as u32 >> 8) as u16;
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
                if state.sample_offset >= sample.length * 2 {
                    state.looped_yet = true;
                    state.sample_offset =
                        sample.repeat_start * 2 + (state.sample_offset - sample.length * 2);
                }
            } else {
                if state.sample_offset >= sample.repeat_start * 2 + sample.repeat_length * 2 {
                    state.sample_offset =
                        state.sample_offset - (sample.repeat_start * 2 + sample.repeat_length * 2);
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

pub fn write_data<T>(
    output: &mut [T],
    channels: usize, /* next_sample: &mut dyn FnMut() -> i16 */
) where
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

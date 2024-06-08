use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{self, Stream};
use eyre::{eyre, ContextCompat, Result};
use modfile::ptmf::{self, Channel, SampleInfo};

use std::env;
use std::fs::File;
use std::io::BufReader;

use once_cell::sync::Lazy;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

static BUFFER: Lazy<Arc<Mutex<VecDeque<i16>>>> =
    Lazy::new(|| Arc::new(Mutex::new(VecDeque::new())));

    #[derive(Debug)]
struct ChannelBuffer {
    data: Vec<u8>,
    curr_offset: u16,
    curr_sample: Option<u8>,
    curr_period: Option<u16>,
    last_sample: Option<u8>,
    last_period: Option<u16>,
    first_iter: bool,
}

impl ChannelBuffer {
    pub fn new() -> Self {
        Self {
            data: vec![0; 600],
            curr_offset: 0,
            curr_sample: None,
            curr_period: None,
            last_sample: None,
            last_period: None,
            first_iter: false,
        }
    }

    pub fn update<'chan>(&mut self, channel: &'chan Channel, sinfo: &Vec<SampleInfo>) {
        // // println!("Update {:?}", channel);
        if channel.period != 0 {
            self.curr_period = Some(channel.period);
            self.curr_offset = 0;
        } else {
            self.curr_period = None;
        }

        if channel.sample_number != 0 {
            self.curr_sample = Some(channel.sample_number);
        } else {
            self.curr_sample = None;
        }

        // // println!("Update {:?}", self);

        // let s_start = if self.first_iter {
        //     sinfo[self.sample() as usize].repeat_start * 2
        // } else {
        //     0
        // };
        let s_len = sinfo[(self.sample() - 1) as usize].length;
        // let s_len = if self.first_iter {
        //     sinfo[self.sample() as usize].repeat_length * 2
        // } else {
        //     sinfo[self.sample() as usize].length
        // };

        let s_data = &sinfo[(self.sample() - 1) as usize].data;
        let samples_this_tick = self.samples_this_tick();

        println!("{}, {}, {}", self.curr_offset, s_len, self.sample());
        if self.curr_offset + samples_this_tick >= s_len {
            
            let (rest, start) = s_data.split_at(self.curr_offset as usize);
            // start.len() does not reflect actual number of samples left.
            let to_end_len = s_len - self.curr_offset;
            // println!("Wrap {} {}", to_end_len, samples_this_tick);
            // // println!("to_end_len: {}", to_end_len);
            let remaining_len = samples_this_tick - to_end_len;

            self.data[0..to_end_len as usize].copy_from_slice(&start[..to_end_len as usize]);
            self.data[to_end_len as usize..(to_end_len + remaining_len) as usize]
                .copy_from_slice(&rest[..remaining_len as usize]);
        } else {
            self.data[0..samples_this_tick as usize].copy_from_slice(
                &s_data[self.curr_offset as usize
                    ..self.curr_offset as usize + samples_this_tick as usize],
            );
        }

        self.curr_offset += self.samples_this_tick();
        self.curr_offset %= s_len;

        if self.curr_period.is_some() {
            self.last_period = self.curr_period;
        }

        if self.curr_sample.is_some() {
            self.last_sample = self.curr_sample;
        }
    }

    fn period(&self) -> u16 {
        // println!("period: {}", self.curr_period.unwrap_or_else(|| self.last_period.unwrap()));
        self.curr_period.unwrap_or_else(|| self.last_period.unwrap())
    }

    fn sample(&self) -> u8 {
        self.curr_sample.unwrap_or_else(|| self.last_sample.unwrap())
    }

    fn freq(&self) -> f32 {
        // FIXME: PAL only.
        7093789.2 / (self.period() as f32 * 2.0)
    }

    pub fn samples_this_tick(&self) -> u16 {
        // FIXME: PAL only.
        // println!("samples_this_tick: {}", (self.freq() / 50.0) as u16);
        (self.freq() / 50.0) as u16
    }
}

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
    let device = host
        .default_output_device()
        .wrap_err("failed to find output device")?;
    let config = device.default_output_config()?;

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => run::<f32>(&device, &config.into()),
        cpal::SampleFormat::I16 => run::<i16>(&device, &config.into()),
        cpal::SampleFormat::U16 => unimplemented!(), /* cpal::SampleFormat::U16 => run::<u16>(&device, &config.into(), module.sample_info), */
    };

    let mut speed = 6;

    let mut channel_bufs = Vec::new();
    for _ in 0..4 {
        channel_bufs.push(ChannelBuffer::new())
    }

    let stream = stream.unwrap();
    stream.play().unwrap();

    for pat in module
        .positions
        .data
        .map(|order| &module.patterns[order as usize]).iter().take(6)
    {
        for row in pat.rows.iter() {
            let mut tick = 0;

            while tick < speed {
                for (cbuf, chan) in channel_bufs.iter_mut().zip(row.channels.iter()) {
                    cbuf.update(chan, &module.sample_info);

                    'outer: loop {
                        let mut deque = BUFFER.lock().unwrap();
                        if deque.len() > 1000 {
                            continue;
                        } else {
                            let samples_this_tick = cbuf.samples_this_tick();
                            // println!("{:?}", &cbuf.data[0..samples_this_tick as usize]);
                            for b in &cbuf.data[0..samples_this_tick as usize] {
                                deque.push_back(*b as i8 as i16);
                            }

                            break 'outer;
                        }
                    }

                    // println!("{:?}, {}", row, tick);
                    // // println!("{:?}, {}, {}, {}, {}, {}, {}, {:?}, {}", row, tick, ch0_hz, ch1_hz, ch2_hz, ch3_hz, ch0_samples_this_tick, channel0, channel0_last_period);
                    tick += 1;
                }
            }
        }
    }

    Ok(())
}

pub fn run<T>(device: &cpal::Device, config: &cpal::StreamConfig) -> Result<Stream>
where
    T: cpal::Sample + From<i16>,
{
    let sample_rate = config.sample_rate.0 as f32;
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

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{self, Stream};
use eyre::Result;
use modfile::ptmf::SampleInfo;
use once_cell::sync::Lazy;

use super::PushSamples;

static BUFFER: Lazy<Arc<Mutex<VecDeque<i16>>>> =
    Lazy::new(|| Arc::new(Mutex::new(VecDeque::new())));


pub struct Sink {
    stream: Stream
}

impl PushSamples for Sink {
    fn push_samples(&mut self, buf: &[i16]) {
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


impl Sink {
    pub fn new<T>(device: &cpal::Device, config: &cpal::StreamConfig) -> Result<Self>
    where T: cpal::Sample + From<i16> {
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

        Ok(Self {
            stream
        })
    }

    pub fn start(&self) -> Result<()> {
        self.stream.play()?;

        Ok(())
    }
}



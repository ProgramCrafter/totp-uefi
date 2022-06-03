#![no_std]
#![no_main]
#![feature(abi_efiapi)]

extern crate alloc;
extern crate uefi;
extern crate uefi_services;

use uefi::proto::console::text::{Input, Key, Output};
use uefi::prelude::*;
use uefi::CStr16;

use core::cmp::min;

use alloc::vec;
use alloc::vec::Vec;
use alloc::boxed::Box;
use alloc::string::{String, ToString};

// big array cannot be allocated on stack
// https://github.com/rust-lang/rust/issues/53827#issuecomment-576450631
macro_rules! box_array {
  ($val:expr ; $len:expr) => {{
    // Use a generic function so that the pointer cast remains type-safe
    fn vec_to_boxed_array<T>(vec: Vec<T>) -> Box<[T; $len]> {
      let boxed_slice = vec.into_boxed_slice();

      let ptr = Box::into_raw(boxed_slice) as *mut [T; $len];

      unsafe { Box::from_raw(ptr) }
    }

    vec_to_boxed_array(vec![$val; $len])
  }};
}

fn delay_approximate(s: usize) {
  let mut v: u8 = 0;
  let p: *mut u8 = &mut v;
  
  for _ in 0..s {
    for _ in 0..256 {
      for _ in 0..1024 {
        for i in 0..255 {
          unsafe {core::ptr::write_volatile(p, i as u8);}
        }
      }
    }
  }
}
fn delay_approximate_ms(ms: usize) {
  let mut v: u8 = 0;
  let p: *mut u8 = &mut v;
  
  for _ in 0..ms {
    for _ in 0..262 {
      for i in 0..255 {
        unsafe {core::ptr::write_volatile(p, i as u8);}
      }
    }
  }
}

fn read_str_from_stdin(stdin: &mut Input) -> String {
  let mut len = 0;
  let mut buf = [' '; 70];
  
  while len < 70 {
    if let Some(k) = stdin.read_key().unwrap() {
      match k {
        Key::Special(_) => {},
        Key::Printable(c) => {
          let c: char = c.into();
          if c == '\r' || c == '\n' {
            break;
          }
          buf[len] = c;
          len += 1;
        }
      }
    }
  }
  
  String::from_iter(buf[0..len].iter())
}

const MEMORY_SIZE: usize = 1048576;

struct Memory<'a> {
  buffer: Box<[u32; MEMORY_SIZE]>,
  io: (&'a mut Input, &'a mut Output<'a>)
}
impl<'a> Memory<'a> {
  fn load32(&self, address: usize) -> u32 {
    self.buffer[address]
  }

  fn load_opcode(&self, address: usize) -> (u16, u16) {
    let i32 = self.load32(address);
    (
      (i32 >> 16).try_into().unwrap(),
      (i32 & 0xFFFF).try_into().unwrap(),
    )
  }

  fn load64(&self, address: usize) -> u64 {
    self.buffer[address * 2] as u64 * 4294967296 + self.buffer[address * 2 + 1] as u64
  }

  fn store64(&mut self, address: usize, value: u64) {
    self.buffer[address * 2] = (value / 4294967296).try_into().unwrap();
    self.buffer[address * 2 + 1] = (value % 4294967296).try_into().unwrap();
  }

  fn store(&mut self, data: &[u8], base: usize) {
    let data_end = min(base + data.len() / 4, MEMORY_SIZE) - 1;
    for i in base..data_end {
      self.buffer[i] = 0;
      for j in 0..4 {
        self.buffer[i] *= 256;
        self.buffer[i] += data[(i - base) * 4 + j] as u32;
      }
    }
  }
}

struct Registers {
  buffer: [i64; 36],
  triggers: Vec<Option<
    (
      Vec<fn(usize, &mut [i64; 36], &mut Memory)>,
      Vec<fn(usize, &mut [i64; 36], &mut Memory)>,
    ),
  >>,
}
impl Registers {
  fn get_triggers_pair(
    &mut self,
    index: usize,
  ) -> &mut (
    Vec<fn(usize, &mut [i64; 36], &mut Memory)>,
    Vec<fn(usize, &mut [i64; 36], &mut Memory)>,
  ) {
    self.triggers[index]
      .get_or_insert((Vec::new(), Vec::new()))
  }

  fn init_triggers(&mut self) {
    fn add_trig(_trig: usize, buffer: &mut [i64; 36], _memory: &mut Memory) {
      buffer[2] = buffer[0] + buffer[1];
    }
    self.get_triggers_pair(0).1.push(add_trig);
    self.get_triggers_pair(1).1.push(add_trig);
    self.get_triggers_pair(2).0.push(add_trig);

    fn sub_trig(_trig: usize, buffer: &mut [i64; 36], _memory: &mut Memory) {
      buffer[5] = buffer[3] - buffer[4];
    }
    self.get_triggers_pair(3).1.push(sub_trig);
    self.get_triggers_pair(4).1.push(sub_trig);
    self.get_triggers_pair(5).0.push(sub_trig);

    fn mul_trig(_trig: usize, buffer: &mut [i64; 36], _memory: &mut Memory) {
      buffer[8] = buffer[6] * buffer[7];
    }
    self.get_triggers_pair(6).1.push(mul_trig);
    self.get_triggers_pair(7).1.push(mul_trig);
    self.get_triggers_pair(8).0.push(mul_trig);

    fn div_trig(_trig: usize, buffer: &mut [i64; 36], _memory: &mut Memory) {
      let div0 = buffer[9];
      let div1 = buffer[10];
      buffer[11] = if div1 != 0 { div0 / div1 } else { div0 };
      buffer[12] = if div1 != 0 { div0 % div1 } else { 0 };
    }
    self.get_triggers_pair(9).1.push(div_trig);
    self.get_triggers_pair(10).1.push(div_trig);
    self.get_triggers_pair(11).0.push(div_trig);
    self.get_triggers_pair(12).0.push(div_trig);

    fn tlt_trig(_trig: usize, buffer: &mut [i64; 36], _memory: &mut Memory) {
      buffer[15] = if buffer[13] < buffer[14] { 1 } else { 0 };
    }
    self.get_triggers_pair(13).1.push(tlt_trig);
    self.get_triggers_pair(14).1.push(tlt_trig);
    self.get_triggers_pair(15).0.push(tlt_trig);

    fn cio_trig(trig: usize, buffer: &mut [i64; 36], memory: &mut Memory) {
      if trig == 1 {
        if buffer[16] == 256 {
          delay_approximate_ms(20);
          memory.io.1.clear().unwrap();
          return;
        }
        
        let mut buf = [0; 3];
        let mut s = String::new();
        if buffer[16] == 10 {s.push('\r');}
        s.push(char::from_u32(buffer[16].try_into().unwrap()).unwrap());
        memory.io.1.output_string(CStr16::from_str_with_buf(&s, &mut buf).unwrap()).unwrap();
      } else {
        buffer[16] = match memory.io.0.read_key().unwrap() {
          None => -1,
          Some(v) => match v {
            Key::Printable(v) => (char::from(v) as u32).into(),
            Key::Special(_) => -1
          }
        };
      }
    }
    self.get_triggers_pair(16).0.push(cio_trig);
    self.get_triggers_pair(16).1.push(cio_trig);

    fn io_trig(trig: usize, buffer: &mut [i64; 36], _memory: &mut Memory) {
      if trig == 1 {
        
      } else {
        buffer[19] = 10;
      }
    }
    self.get_triggers_pair(18).1.push(io_trig);
    self.get_triggers_pair(19).0.push(io_trig);

    fn atz_trig(_trig: usize, buffer: &mut [i64; 36], _memory: &mut Memory) {
      buffer[23] = if buffer[20] == 0 {
        buffer[21]
      } else {
        buffer[22]
      };
    }
    self.get_triggers_pair(20).1.push(atz_trig);
    self.get_triggers_pair(21).1.push(atz_trig);
    self.get_triggers_pair(22).1.push(atz_trig);
    self.get_triggers_pair(23).0.push(atz_trig);

    fn mem_trig(trig: usize, buffer: &mut [i64; 36], memory: &mut Memory) {
      if trig == 1 {
        memory.store64(
          buffer[26].try_into().unwrap(),
          buffer[24].try_into().unwrap(),
        );
      } else {
        buffer[24] = memory
          .load64(buffer[26].try_into().unwrap())
          .try_into()
          .unwrap();
      }
    }
    self.get_triggers_pair(24).0.push(mem_trig);
    self.get_triggers_pair(24).1.push(mem_trig);
    self.get_triggers_pair(26).1.push(mem_trig);
  }

  fn set(&mut self, index: usize, value: i64, memory: &mut Memory) {
    self.buffer[index] = value;

    match &self.triggers.get(index) {
      None => {},
      Some(v) => match v {
        Some(trigs) => {
          let buf = &mut (self.buffer);
          for callback in trigs.1.iter() {
            callback(1, buf, memory);
          }
        },
        None => {}
      }
    }
  }

  fn get(&mut self, index: usize, memory: &mut Memory) -> i64 {
    match &self.triggers.get(index) {
      None => {},
      Some(v) => match v {
        Some(trigs) => {
          let buf = &mut (self.buffer);
          for callback in trigs.0.iter() {
            callback(0, buf, memory);
          }
        },
        None => {}
      }
    }
    self.buffer[index]
  }
}

#[entry]
fn efi_main(image_handle: uefi::Handle, mut system_table: SystemTable<Boot>) -> Status {
  if let Err(_) = uefi_services::init(&mut system_table) {return Status::LOAD_ERROR;}
  
  let mut system_table_stdin = unsafe {system_table.unsafe_clone()};
  let mut system_table_stdout = unsafe {system_table.unsafe_clone()};
  
  let stdout = system_table_stdout.stdout();
  let stdin = system_table_stdin.stdin();
  if stdout.reset(true).is_err() {return Status::LOAD_ERROR;}
  if stdin.reset(true).is_err() {return Status::LOAD_ERROR;}
  
  let mut regs: Registers = Registers {
    buffer: [0; 36],
    triggers: vec![None; 27],
  };

  let mut mem: Memory = Memory {
    buffer: box_array![0; MEMORY_SIZE],
    io: (stdin, stdout),
  };

  regs.init_triggers();
  mem.store(b"\x80<\x00\x1c\x80\n\x00\x1b\x80\x01\x00\x04\x00\x03\x00\x14\x00\x1d\x00\x15\x80\x07\x00\x16\x00\x17\x00\x1b\x00\x1e\x00\x10\x00\x05\x00\x03\x80\x03\x00\x1b\x00\x10\x00\x03\x80 \x00\x04\x00\x05\x00\x14\x80N\x00\x15\x80\x10\x00\x16\x00\x17\x00\x1b\xa5T\x00\x10\x00\x1c\x00\x03\x80\x02\x00\x04\x00\x05\x00\x03\xa5P\x00\x1e\x80\x17\x00\x1d\x80\x02\x00\x1b\xa5W\x00\x10\x80\n\x00\x10\xa5Q\x00\x10\x80 \x00\x10\x80 \x00\x10\x80T\x00\x10\x80i\x00\x10\x80g\x00\x10\x80e\x00\x10\x80r\x00\x10\x80O\x00\x10\x80S\x00\x10\x80 \x00\x10\x80v\x00\x10\x800\x00\x10\x80.\x00\x10\x800\x00\x10\x80.\x00\x10\x801\x00\x10\x80 \x00\x10\x80|\x00\x10\x80 \x00\x10\x80n\x00\x10\x80o\x00\x10\x80t\x00\x10\x80 \x00\x10\x80l\x00\x10\x80i\x00\x10\x80c\x00\x10\x80e\x00\x10\x80n\x00\x10\x80s\x00\x10\x80e\x00\x10\x80d\x00\x10\x80!\x00\x10\x80!\x00\x10\x80!\x00\x10\x00\x1c\x00\x03\x80$\x00\x04\x00\x05\x00\x03\x80 \x00\x1e\x80B\x00\x1d\x80\x02\x00\x1b\xa5Q\x00\x10\x80\n\x00\x10\xa5Z\x00\x10\x00\x1c\x00\x03\x80\x02\x00\x04\x00\x05\x00\x03\xa5P\x00\x1e\x80K\x00\x1d\x80\x02\x00\x1b\xa5]\x00\x10\x81\x00\x00\x10\x80\n\x00\x1b\x81\x01\x00\x10", 0);
  
  loop {
    let addr = regs.buffer[27] as usize;

    if addr >= MEMORY_SIZE {
      break;
    }

    let (src, dst) = mem.load_opcode(addr);

    let val = if src & 0x8000 != 0 {
      (src & 0x7FFF) as i64
    } else {
      regs.get(src.try_into().unwrap(), &mut mem)
    };
    regs.buffer[27] = (addr + 1).try_into().unwrap();
    regs.set(dst.try_into().unwrap(), val, &mut mem);
  }
  
  delay_approximate(3);
  
  let (stdin, stdout) = mem.io;
  drop(stdin);
  
  let mut buf = [0; 81];
  
  let handles = system_table.boot_services().locate_handle_buffer(
    uefi::table::boot::SearchType::AllHandles).unwrap();
  
  stdout.output_string(
    CStr16::from_str_with_buf("Handles count: ", &mut buf).unwrap()).unwrap();
  stdout.output_string(
    CStr16::from_str_with_buf(&handles.handles().len().to_string(), &mut buf).unwrap()).unwrap();
  stdout.output_string(
    CStr16::from_str_with_buf("\n", &mut buf).unwrap()).unwrap();
  
  let go_handles = system_table.boot_services()
    .find_handles::<uefi::proto::console::gop::GraphicsOutput>().unwrap();
  
  if go_handles.is_empty() {return Status::LOAD_ERROR;}
  
  {
    let go_scoped: uefi::table::boot::ScopedProtocol<'_, uefi::proto::console::gop::GraphicsOutput> =
      system_table.boot_services().open_protocol(
        uefi::table::boot::OpenProtocolParams {handle: go_handles[0], agent: image_handle, controller: None},
        uefi::table::boot::OpenProtocolAttributes::Exclusive
      ).unwrap();
    let go = unsafe {&mut *go_scoped.interface.get()};
    
    let (width, height) = go.current_mode_info().resolution();
    let stride = go.current_mode_info().stride();
    
    let bpp = go.current_mode_info().pixel_format();
    let color = match bpp {
      uefi::proto::console::gop::PixelFormat::Rgb => {0xFF800000},
      uefi::proto::console::gop::PixelFormat::Bgr => {0x80FF0000},
      _ => {return Status::LOAD_ERROR}
    };
    
    let mut fb = go.frame_buffer();
    
    for i in 0..min(width, height) {
      let id = i * stride + i;
      unsafe {
        fb.write_value::<u32>(id * 4, color);
      }
    }
  }
  
  // delay_approximate(5);
  
  Status::LOAD_ERROR
}

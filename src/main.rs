#![no_std]
#![no_main]
#![feature(abi_efiapi)]
#![feature(exclusive_range_pattern)]

extern crate alloc;
extern crate uefi;
extern crate uefi_services;

use uefi::table::boot::{ScopedProtocol, OpenProtocolParams, OpenProtocolAttributes};
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat, FrameBuffer};
use uefi::prelude::*;
use uefi::CStr16;

use alloc::vec::Vec;

fn delay(system_table: &mut SystemTable<Boot>, s: usize) {
  system_table.boot_services().stall(s * 1000 * 1000);
}

fn delay_approximate(ms: usize) {
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

struct WindowManager<'boot> {
  windows:  Vec<Window>,
  width:    usize,
  height:   usize,
  stride:   usize,
  fb:       FrameBuffer<'boot>,
  _proto:   ScopedProtocol<'boot, GraphicsOutput<'boot>>
}

struct Window {
  x:        usize,
  y:        usize,
  w:        usize,
  h:        usize,
  color:    u32
}

impl<'boot> WindowManager<'boot> {
  fn new(system_table: &'boot mut SystemTable<Boot>, image_handle: uefi::Handle) -> Option<Self> {
    let go_handles = system_table.boot_services().find_handles::<GraphicsOutput>().unwrap();
    
    if go_handles.is_empty() {return None;}
    
    let go_scoped: ScopedProtocol<'_, GraphicsOutput> = match system_table.boot_services()
      .open_protocol(
        OpenProtocolParams {handle: go_handles[0], agent: image_handle, controller: None},
        OpenProtocolAttributes::Exclusive
      ) {
      Ok(v) => {v},
      Err(_) => {return None}
    };
    
    let go = unsafe {&mut *go_scoped.interface.get()};
    
    let (width, height) = go.current_mode_info().resolution();
    let stride = go.current_mode_info().stride();
    
    if match go.current_mode_info().pixel_format() {
      PixelFormat::Rgb | PixelFormat::Bgr => {false},
      _ => {true}
    } {return None;}
    
    let mut fb = go.frame_buffer();
    
    for i in 2..80 {
      unsafe {
        fb.write_value::<u32>((i * stride + i) * 4, 0x000000FF);
      }
    }
    
    Some(WindowManager {
      windows: Vec::new(),
      width:   width,
      height:  height,
      stride:  stride,
      fb:      fb,
      _proto:  go_scoped
    })
  }
  
  #[allow(unused)]
  fn draw_pixel(&mut self, x: usize, y: usize, color: u32) {
    if x >= self.width || y >= self.height {return;}
    
    unsafe {
      self.fb.write_value::<u32>(y * self.stride + x, color);
    }
  }
  
  fn draw_windows(&mut self) {
    for window in self.windows.iter() {
      for y in window.y..(window.y + window.h) {
        for x in window.x..(window.x + window.w) {
          if x >= self.width || y >= self.height {continue;}
          
          unsafe {
            self.fb.write_value::<u32>((y * self.stride + x) * 4, window.color);
          }
        }
      }
    }
  }
  
  fn tick(&mut self, system_table: &mut SystemTable<Boot>) {
    self.draw_windows();
    
    delay_approximate(40);
    // system_table.boot_services().stall(50); // 50 ms
  }
}

#[entry]
fn efi_main(image_handle: uefi::Handle, mut system_table: SystemTable<Boot>) -> Status {
  if let Err(_) = uefi_services::init(&mut system_table) {return Status::LOAD_ERROR;}
  
  let mut buf = [0; 255];
  let mut system_table_stdout = unsafe {system_table.unsafe_clone()};
  system_table_stdout.stdout().output_string(CStr16::from_str_with_buf("ZZZZZ", &mut buf).unwrap()).unwrap();
  
  let wm = WindowManager::new(&mut system_table, image_handle);
  let mut wm = match wm {
    Some(v) => {v},
    None => {return Status::LOAD_ERROR}
  };
  
  wm.windows.push(Window {x: 0, y: 0, w: 300, h: 200, color: 0x0000FF00});
  
  let mut vx: i32 = 1;
  let mut vy: i32 = 1;
  let mut i = 0;
  loop {
    wm.windows[0].x = ((wm.windows[0].x as i32) + vx) as usize;
    wm.windows[0].y = ((wm.windows[0].y as i32) + vy) as usize;
    if wm.windows[0].x + 300 >= wm.width - 1 {vx = -1;}
    if wm.windows[0].x == 0 {vx = 1;}
    if wm.windows[0].y + 200 >= wm.height - 1 {vy = -1;}
    if wm.windows[0].y == 0 {vy = 1;}
    
    i += 1;
    i %= 256 * 8;
    
    wm.windows[0].color = match i % 2048 {
      0..256 =>     {0x00FFFFFF},
      256..512 =>   {(i % 256) * 0x00000001},
      512..768 =>   {(i % 256) * 0x00000100},
      768..1024 =>  {(i % 256) * 0x00000101},
      1024..1280 => {(i % 256) * 0x00010000},
      1280..1536 => {(i % 256) * 0x00010001},
      1536..1792 => {(i % 256) * 0x00010100},
      1792..2048 => {(i % 256) * 0x00010101},
      _ => {unreachable!()}
    };
    
    wm.tick(&mut system_table_stdout);
  }
}

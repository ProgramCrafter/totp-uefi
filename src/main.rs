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
    
    system_table.boot_services().stall(50000); // 50'000 mcs
    
    while system_table.stdin().read_key().unwrap().is_none() {
      system_table.boot_services().stall(50000);
    }
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
  
  wm.windows.push(Window {x: 0, y: 0, w: wm.width, h: wm.height, color: 0x0000FF00});
  
  let mut i = 0;
  loop {
    i = (i + 1) % 16;
    
    wm.windows[0].color = match i {
      0 => {0x0000FF00},
      1 => {0x0000FFFF},
      2 => {0x000000FF},
      3 => {0x00FF00FF},
      4 => {0x00FF0000},
      5 => {0x00FFFF00},
      6 => {0x00FFFFFF},
      7 => {0x00000000},
      8  => {0x00008000},
      9  => {0x00008080},
      10 => {0x00000080},
      11 => {0x00800080},
      12 => {0x00800000},
      13 => {0x00808000},
      14 => {0x00808080},
      15 => {0x00404040},
      _ => {unreachable!()}
    };
    
    wm.tick(&mut system_table_stdout);
  }
}

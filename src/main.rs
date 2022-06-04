#![no_std]
#![no_main]
#![feature(abi_efiapi)]

extern crate alloc;
extern crate uefi;
extern crate uefi_services;

use uefi::table::boot::{ScopedProtocol, OpenProtocolParams, OpenProtocolAttributes};
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat, FrameBuffer};
use uefi::prelude::*;
use uefi::CStr16;

use alloc::vec::Vec;
use alloc::string::ToString;

fn delay(system_table: &mut SystemTable<Boot>, s: usize) {
  system_table.boot_services().stall(s * 1000 * 1000);
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
    
    delay_approximate(5);
    
    Some(WindowManager {
      windows: Vec::new(),
      width:   width,
      height:  height,
      stride:  stride,
      fb:      fb,
      _proto:  go_scoped
    })
  }
  
  /*
  fn draw_pixel(&mut self, x: usize, y: usize, color: u32) {
    if x >= self.width || y >= self.height {return;}
    
    unsafe {
      self.fb.write_value::<u32>(y * self.stride + x, color);
    }
  }
  */
  
  fn draw_windows(&mut self, stdout: &mut uefi::proto::console::text::Output) {
    let mut buf = [0; 255];
    
    for window in self.windows.iter() {
      stdout.output_string(CStr16::from_str_with_buf(
        &("Drawing window ".to_string() + &window.x.to_string() + "+" + &window.y.to_string() + "+" +
        &window.w.to_string() + "x" + &window.h.to_string()), &mut buf).unwrap()).unwrap();
      
      /*
      for y in window.y..(window.y + window.h) {
        for x in window.x..(window.x + window.w) {
          if x >= self.width || y >= self.height {continue;}
          
          unsafe {
            self.fb.write_value::<u32>(y * self.stride + x, window.color);
          }
        }
      }
      */
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
  
  wm.windows.push(Window {x: 0, y: 0, w: 300, h: 200, color: 0x0000FF00});
  wm.draw_windows(&mut system_table_stdout.stdout());
  
  delay_approximate(3);
  
  // delay(&mut system_table_stdout, 5);
  // wm.windows[0].color = 0x0080FF00;
  // wm.draw_windows();
  
  // delay_approximate(5);
  
  Status::LOAD_ERROR
}

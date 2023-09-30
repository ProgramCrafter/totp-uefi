#![no_std]
#![no_main]

extern crate alloc;
extern crate uefi;
extern crate uefi_services;

use alloc::string::{ToString, String};
use uefi::proto::console::text::Key::{Printable, Special};
use uefi::proto::console::text::ScanCode;
use chrono::{FixedOffset, TimeZone};
use uefi::prelude::*;
use uefi::CStr16;

use totp_rs::{Algorithm, TOTP, Secret};


macro_rules! print_str {
    ($system_table:expr, $str:expr) => {
        let mut buf = [0; 255];
        let s = CStr16::from_str_with_buf($str, &mut buf).unwrap();
        $system_table.stdout().output_string(s).unwrap();
    }
}


struct TotpState {
    secret: String,
    totp:   Option<TOTP>,
}
impl TotpState {
    fn print_key(&self, table: &mut SystemTable<Boot>, split: usize) {
        if self.secret.is_empty() {return;}
        
        for i in 0..(self.secret.len()-1)/split {
            print_str!(table, &self.secret[i*split..(i+1)*split]);
            print_str!(table, "-");
        }
        print_str!(table, &self.secret[(self.secret.len()-1)/split*split..]);
    }
    
    fn draw(&self, table: &mut SystemTable<Boot>) {
        table.stdout().clear().unwrap();
        print_str!(table, "Secret: ");
        self.print_key(table, 4);
        print_str!(table, "\r\nKey: ");
        match &self.totp {
            None    => { print_str!(table, "null"); }
            Some(t) => {
                let token = t.generate(self.get_time(table));
                print_str!(table, &token);
            }
        }
    }
    
    fn get_time(&self, table: &mut SystemTable<Boot>) -> u64 {
        let uefi_time = table.runtime_services().get_time().expect("timer failure");
        let uefi_offset_available = uefi_time.time_zone().is_some();
        let uefi_offset = uefi_time.time_zone().map_or(5 * 3600, |tz| tz * 60);
        let chrono_offset = FixedOffset::east_opt(uefi_offset.into()).unwrap();
        let chrono_time = chrono_offset.with_ymd_and_hms(
            uefi_time.year() as i32, uefi_time.month() as u32,  uefi_time.day() as u32,
            uefi_time.hour() as u32, uefi_time.minute() as u32, uefi_time.second() as u32
        ).single().expect("UEFI returned invalid time");
        
        print_str!(table, " [");
        if !uefi_offset_available { print_str!(table, "INACCURATE_OFFSET "); }
        print_str!(table, &chrono_time.to_string());
        print_str!(table, "] ");
        return chrono_time.timestamp() as u64;
    }
    
    fn update_secret(&mut self, table: &mut SystemTable<Boot>) {
        if let Some(key) = table.stdin().read_key().expect("input device failure") {
            match key {
                Printable(c) => {
                    if u16::from(c) == 8 {
                        if self.secret.is_empty() {return;}
                        self.secret.pop();
                    } else {
                        let c = char::from(c).to_uppercase().to_string();
                        
                        if self.secret.len() + c.len() > 60 {return;}
                        if !"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567".contains(&c) {return;}
                        self.secret += &c;
                    }
                },
                Special(ScanCode(_)) => {}
            }
            
            let secret = Secret::Encoded(self.secret.clone()).to_bytes();
            self.totp = secret.ok().map(|s| {
                TOTP::new(Algorithm::SHA1, 6, 1, 30, s).ok()
            }).flatten();
        }
    }
}


#[entry]
fn efi_main(_image_handle: uefi::Handle, mut system_table: SystemTable<Boot>) -> Status {
    if let Err(_) = uefi_services::init(&mut system_table) {
        return Status::LOAD_ERROR;
    }
    
    let mut totp = TotpState {secret: String::new(), totp: None};
    
    loop {
        totp.draw(&mut system_table);
        totp.update_secret(&mut system_table);
        system_table.boot_services().stall(50000);  // 50 ms
    }
}

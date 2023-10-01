#![no_std]
#![no_main]

extern crate alloc;
extern crate uefi;
extern crate uefi_services;

use uefi::proto::console::text::Key::{Printable, Special};
use uefi::table::runtime::VariableAttributes;
use uefi::proto::console::text::ScanCode;
use alloc::string::{ToString, String};
use chrono::{FixedOffset, TimeZone};
use uefi::prelude::*;
use alloc::vec::Vec;
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
    fn new(secret: String) -> TotpState {
        let mut this = TotpState {secret: secret, totp: None};
        this.update_totp();
        this
    }
    
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
    
    fn update_secret(&mut self, table: &mut SystemTable<Boot>) -> bool {
        if let Some(key) = table.stdin().read_key().expect("input device failure") {
            match key {
                Printable(c) => {
                    if u16::from(c) == 8 {
                        if self.secret.is_empty() {return false;}
                        self.secret.pop();
                    } else {
                        let c = char::from(c).to_uppercase().to_string();
                        
                        if self.secret.len() + c.len() > 60 {return false;}
                        if !"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567".contains(&c) {return false;}
                        self.secret += &c;
                    }
                },
                Special(ScanCode(_)) => {return false;}
            }
            
            self.update_totp();
            true
        } else {
            false
        }
    }
    
    fn update_totp(&mut self) {
        let secret = Secret::Encoded(self.secret.clone()).to_bytes();
        self.totp = secret.ok().map(|s| {
            TOTP::new(Algorithm::SHA1, 6, 1, 30, s).ok()
        }).flatten();
    }
}


fn load_secret(table: &mut SystemTable<Boot>) -> String {
    let mut vn_buf = [0; 255];
    let var_name = CStr16::from_str_with_buf("totp_key", &mut vn_buf).unwrap();
    
    let guid = uefi::Guid::parse_or_panic("572e6927-177b-49ce-b761-2cdc60f42491");
    let vendor = uefi::table::runtime::VariableVendor(guid);
    
    match table.runtime_services().get_variable_boxed(&var_name, &vendor).ok() {
        Some((var_box, _attr)) => {
            let var_bytes = Vec::from(var_box);
            match String::from_utf8(var_bytes).ok() {
                Some(s) => s,
                None => String::new()
            }
        },
        None => String::new()
    }
}

fn save_secret(table: &mut SystemTable<Boot>, key: &str) {
    let mut vn_buf = [0; 255];
    let var_name = CStr16::from_str_with_buf("totp_key", &mut vn_buf).unwrap();
    
    let guid = uefi::Guid::parse_or_panic("572e6927-177b-49ce-b761-2cdc60f42491");
    let vendor = uefi::table::runtime::VariableVendor(guid);
    
    let attr = VariableAttributes::NON_VOLATILE | VariableAttributes::BOOTSERVICE_ACCESS;
    
    table.runtime_services().set_variable(&var_name, &vendor, attr, key.as_bytes()).unwrap();
}


#[entry]
fn efi_main(_image_handle: uefi::Handle, mut system_table: SystemTable<Boot>) -> Status {
    if let Err(_) = uefi_services::init(&mut system_table) {
        return Status::LOAD_ERROR;
    }
    
    let mut totp = TotpState::new(load_secret(&mut system_table));
    
    system_table.boot_services().set_watchdog_timer(0, 0x10000, None).unwrap();
    
    loop {
        totp.draw(&mut system_table);
        if totp.update_secret(&mut system_table) {
            save_secret(&mut system_table, &totp.secret);
        }
        
        system_table.boot_services().stall(50000);  // 50 ms
    }
}

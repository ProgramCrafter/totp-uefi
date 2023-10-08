cargo +nightly build --target x86_64-unknown-uefi -r
copy D:\Rust\totp-uefi\target\x86_64-unknown-uefi\release\totp-uefi.efi D:\Rust\totp-uefi\target\share\efi\boot\bootx64.efi >nul
@if $%1==$nopause goto end
@pause
@:end

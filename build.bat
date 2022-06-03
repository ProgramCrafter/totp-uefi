cargo +nightly build --target x86_64-unknown-uefi
copy D:\Rust\mov-uefi\target\x86_64-unknown-uefi\debug\mov-uefi.efi D:\Rust\mov-uefi\target\share\efi\boot\bootx64.efi >nul
@if $%1==$nopause goto end
@pause
@:end

# totp-uefi
This is a TOTP implementation that is compiled into single EFI file, that can be launched on a computer or in virtual machine. Thus, it can be launched without an operating system.
<br>Its other features are:
<ol>
  <li>Different key lengths are supported.</li>
  <li>Code is generated and updated instantly.</li>
  <li>Key is preserved across reboots (you can use build from commit 20f545d if you don't want this).</li>
</ol>
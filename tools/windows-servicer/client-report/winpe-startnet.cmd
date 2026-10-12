@echo off
rem WinPE startup for the NIC inventory image (boot-inventory.wim).
rem Runs the inventory reporter, then reboots back to the normal boot menu
rem whether the upload succeeded or failed.
wpeinit
powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%SYSTEMROOT%\winpe-inventory.ps1"
wpeutil reboot

#!/bin/bash
set -e
DISK=$1

if [ -z "$DISK" ]; then
    echo "Usage: $0 /dev/diskN"
    echo "Find your USB disk with: diskutil list"
    exit 1
fi

echo "Building Maya USB image on $DISK"
echo "Press Ctrl+C within 5 seconds to cancel..."
sleep 5

# Unmount if mounted
diskutil unmountDisk $DISK 2>/dev/null || true

# Create GPT partition table with EFI partition
diskutil partitionDisk $DISK GPT \
    "EFI FAT32" MAYA_EFI 200MB \
    free free 0

# Wait for partition to appear
sleep 2

# Mount EFI partition
EFI_PART="${DISK}s1"
mkdir -p /tmp/maya-usb
mount -t msdos $EFI_PART /tmp/maya-usb

# Create directory structure
mkdir -p /tmp/maya-usb/EFI/BOOT

# Copy bootloader
BOOTLOADER=$(find /Users/buddhi/aios/target/debug/build \
    -name "bootloader-x86_64-uefi.efi" | head -1)
cp $BOOTLOADER /tmp/maya-usb/EFI/BOOT/BOOTX64.EFI

# Copy kernel
cp /Users/buddhi/Desktop/maya/target/x86_64-aios/debug/kernel \
    /tmp/maya-usb/kernel-x86_64

# Copy startup script
printf 'EFI\\BOOT\\BOOTX64.EFI\r\n' \
    > /tmp/maya-usb/startup.nsh

# Verify files
echo "Files on USB:"
ls -lh /tmp/maya-usb/EFI/BOOT/
ls -lh /tmp/maya-usb/kernel-x86_64

# Sync and unmount
sync
umount /tmp/maya-usb
diskutil eject $DISK

echo ""
echo "Maya USB ready."
echo "Boot the Dell from this USB."
echo "In Dell BIOS: F12 for boot menu, select USB."

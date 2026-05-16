#![allow(dead_code)]

use core::{
    arch::{asm, global_asm},
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
use x86_64::{
    PhysAddr, VirtAddr,
    structures::paging::{Page, PageTableFlags, PhysFrame, Size4KiB},
};

use crate::{
    fb_print,
    memory::{pmm, vmm},
    sched::queue,
    serial_print,
    serial_print_usize,
};

#[derive(Debug, Clone, Copy)]
pub struct CpuInfo {
    pub apic_id: u8,
    pub processor_id: u8,
    pub enabled: bool,
    pub mailbox_addr: u64,
}

const EMPTY_CPU_INFO: CpuInfo = CpuInfo {
    apic_id: 0,
    processor_id: 0,
    enabled: false,
    mailbox_addr: 0,
};
const MAX_CPUS: usize = 16;
const SDT_HEADER_SIZE: usize = 36;
const MADT_HEADER_EXTRA: usize = 8;
const ACPI_MADT_SIGNATURE: &[u8; 4] = b"APIC";
const APIC_BASE: u64 = 0xFEE00000;
const APIC_ICR_LOW: u32 = 0x300;
const APIC_ICR_HIGH: u32 = 0x310;
const TRAMPOLINE_ADDR: u32 = 0x8000;
const DATA_BASE: usize = 0x8200;
const TRAMPOLINE_DATA_AP_ENTRY_OFFSET: usize = 0;
const TRAMPOLINE_DATA_CR3_OFFSET: usize = 8;
const TRAMPOLINE_DATA_STACKS_OFFSET: usize = 16;
const TRAMPOLINE_DATA_READY_OFFSET: usize = 144;
const AP_STACK_FRAMES: usize = 4;
const PARKING_PROTOCOL_VERSION: u32 = 0;

#[repr(C)]
struct ParkingMailbox {
    processor_id: u32,
    reserved: u32,
    wakeup_vector: u64,
}

#[repr(C, packed)]
struct RsdpV1 {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_address: u32,
}

#[repr(C, packed)]
struct RsdpV2 {
    first_part: RsdpV1,
    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    reserved: [u8; 3],
}

#[repr(C, packed)]
struct AcpiSdtHeader {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}

#[repr(C, packed)]
struct MadtEntryHeader {
    entry_type: u8,
    length: u8,
}

static mut CPU_COUNT: usize = 1;
static mut CPU_LIST: [CpuInfo; 16] = [EMPTY_CPU_INFO; 16];
static AP_ONLINE: [AtomicBool; 16] = [const { AtomicBool::new(false) }; 16];

unsafe extern "C" {
    static ap_trampoline_start: u8;
    static ap_trampoline_end: u8;
}

global_asm!(
    r#"
    .section .text
    .global ap_trampoline_start
    .global ap_trampoline_end

    .code16
ap_trampoline_start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00
    .byte 0x0F, 0x01, 0x16
    .word 0x8300
    mov eax, cr0
    or eax, 0x1
    mov cr0, eax
    .byte 0x66, 0xea
    .long 0x8080
    .word 0x08

    .org ap_trampoline_start + 0x40
    .code32
ap_protected_mode:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov eax, cr4
    or eax, 0x20
    mov cr4, eax
    mov eax, dword ptr [0x8208]
    mov cr3, eax
    mov ecx, 0xC0000080
    rdmsr
    or eax, 0x100
    wrmsr
    mov eax, cr0
    or eax, 0x80000000
    mov cr0, eax
    .byte 0x66, 0xea
    .long 0x8100
    .word 0x18

    .org ap_trampoline_start + 0x80
    .code64
ap_long_mode:
    mov ax, 0x20
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov rax, 0xFEE00020
    mov eax, dword ptr [rax]
    shr eax, 24
    movzx rcx, al
    mov rax, 0x8210
    mov rsp, qword ptr [rax + rcx*8]
    mov rax, 0x8290
    mov byte ptr [rax + rcx], 1
    mov rax, qword ptr [0x8200]
    jmp rax
ap_trampoline_end:
"#
);

pub fn detect_cpus(rsdp_addr: u64) -> usize {
    let count = unsafe { detect_cpus_inner(rsdp_addr).unwrap_or(1) };
    unsafe {
        CPU_COUNT = count;
    }
    count
}

pub fn cpu_count() -> usize {
    unsafe { CPU_COUNT }
}

pub fn bsp_apic_id() -> u8 {
    unsafe {
        let apic = 0xFEE00020 as *const u32;
        ((apic.read_volatile() >> 24) & 0xFF) as u8
    }
}

pub fn start_aps() {
    start_aps_parking_protocol();
}

pub fn start_aps_parking_protocol() {
    let stack_page = Page::<Size4KiB>::containing_address(VirtAddr::new(0x7000));
    let stack_frame = match PhysFrame::from_start_address(PhysAddr::new(0x7000)) {
        Ok(frame) => frame,
        Err(_) => return,
    };
    let _ = vmm::map_page(
        stack_page,
        stack_frame,
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
    );
    let trampoline_page =
        Page::<Size4KiB>::containing_address(VirtAddr::new(TRAMPOLINE_ADDR as u64));
    let trampoline_frame =
        match PhysFrame::from_start_address(PhysAddr::new(TRAMPOLINE_ADDR as u64)) {
            Ok(frame) => frame,
            Err(_) => return,
        };
    let _ = vmm::map_page(
        trampoline_page,
        trampoline_frame,
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
    );
    let (code_start, total_len) = {
        let start = ptr::addr_of!(ap_trampoline_start) as usize;
        let end = ptr::addr_of!(ap_trampoline_end) as usize;
        (start, end - start)
    };

    unsafe {
        ptr::copy_nonoverlapping(
            code_start as *const u8,
            TRAMPOLINE_ADDR as *mut u8,
            total_len,
        );
    }
    unsafe {
        let gdt = 0x8310usize as *mut u8;
        for i in 0..8 {
            gdt.add(i).write_volatile(0);
        }
        let e1: [u8; 8] = [0xFF, 0xFF, 0x00, 0x00, 0x00, 0x9A, 0xCF, 0x00];
        for i in 0..8 {
            gdt.add(8 + i).write_volatile(e1[i]);
        }
        let e2: [u8; 8] = [0xFF, 0xFF, 0x00, 0x00, 0x00, 0x92, 0xCF, 0x00];
        for i in 0..8 {
            gdt.add(16 + i).write_volatile(e2[i]);
        }
        let e3: [u8; 8] = [0xFF, 0xFF, 0x00, 0x00, 0x00, 0x9A, 0xAF, 0x00];
        for i in 0..8 {
            gdt.add(24 + i).write_volatile(e3[i]);
        }
        let e4: [u8; 8] = [0xFF, 0xFF, 0x00, 0x00, 0x00, 0x92, 0xAF, 0x00];
        for i in 0..8 {
            gdt.add(32 + i).write_volatile(e4[i]);
        }

        let ptr = 0x8300usize as *mut u8;
        ptr.add(0).write_volatile(0x27u8);
        ptr.add(1).write_volatile(0x00u8);
        ptr.add(2).write_volatile(0x10u8);
        ptr.add(3).write_volatile(0x82u8);
        ptr.add(4).write_volatile(0x00u8);
        ptr.add(5).write_volatile(0x00u8);
    }

    let data_ptr = DATA_BASE as *mut u8;
    let cr3: u64;
    unsafe {
        asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags));
    }

    unsafe {
        let data = DATA_BASE as *mut u64;
        data.write(ap_entry as *const () as u64);
        data.add(1).write(cr3);
        for i in 0..MAX_CPUS {
            ((0x8190usize + i) as *mut u8).write(0u8);
        }
    }

    for index in 0..MAX_CPUS {
        AP_ONLINE[index].store(false, Ordering::Release);
        write_trampoline_u64(data_ptr, TRAMPOLINE_DATA_STACKS_OFFSET + index * 8, 0);
        write_trampoline_u8(data_ptr, TRAMPOLINE_DATA_READY_OFFSET + index, 0);
    }

    let bsp_id = bsp_apic_id();
    let cpu_count = unsafe { CPU_COUNT };
    serial_print("SMP: parking protocol start\n");
    serial_print("SMP: starting ");
    serial_print_usize(cpu_count.saturating_sub(1));
    serial_print(" APs\n");

    for idx in 0..cpu_count {
        let apic_id = unsafe { CPU_LIST[idx].apic_id };
        let enabled = unsafe { CPU_LIST[idx].enabled };
        let mailbox_phys = unsafe { CPU_LIST[idx].mailbox_addr };
        if !enabled || apic_id == bsp_id {
            continue;
        }

        serial_print("SMP: cpu ");
        serial_print_usize(apic_id as usize);
        serial_print(" mailbox=");
        serial_print_usize(mailbox_phys as usize);
        serial_print("\n");

        let stack_top = match allocate_ap_stack_top() {
            Some(top) if top != 0 => top,
            _ => {
                serial_print("SMP: timeout core ");
                serial_print_usize(apic_id as usize);
                serial_print("\n");
                continue;
            }
        };
        unsafe {
            let stack_slot = (0x8110 + apic_id as usize * 8) as *mut u64;
            stack_slot.write(stack_top);
            ((0x8190usize + apic_id as usize) as *mut u8).write(0u8);
        }

        if mailbox_phys != 0 {
            serial_print("SMP: using parking protocol\n");
            start_ap_parking_protocol(apic_id, mailbox_phys);
        } else {
            serial_print("SMP: falling back to SIPI\n");
            start_ap_sipi(apic_id);
        }
        if !wait_for_ap_ready(apic_id) {
            serial_print("SMP: timeout core ");
            serial_print_usize(apic_id as usize);
            serial_print("\n");
        } else {
            serial_print("AP online: core ");
            serial_print_usize(apic_id as usize);
            serial_print("\n");
        }
    }
}

pub extern "C" fn ap_entry() -> ! {
    let apic_id = unsafe {
        let ptr = (0xFEE00020u64) as *const u32;
        ((ptr.read_volatile() >> 24) & 0xFF) as u8
    };
    serial_print("AP online: core ");
    serial_print_usize(apic_id as usize);
    serial_print("\n");
    AP_ONLINE[apic_id as usize].store(true, Ordering::Release);
    queue::init_core(apic_id);
    crate::sched::timer::init_ap_timer(apic_id);
    unsafe {
        asm!("sti", options(nomem, nostack, preserves_flags));
    }
    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

pub extern "C" fn ap_entry_parking() -> ! {
    let apic_id = unsafe {
        let ptr = 0xFEE00020u64 as *const u32;
        ((ptr.read_volatile() >> 24) & 0xFF) as u8
    };
    serial_print("AP online: core ");
    serial_print_usize(apic_id as usize);
    serial_print("\n");
    fb_print("AP online: core ");
    crate::fb_print_usize(apic_id as usize);
    fb_print("\n");
    queue::init_core(apic_id);
    crate::sched::timer::init_ap_timer(apic_id);
    AP_ONLINE[apic_id as usize].store(true, Ordering::Release);
    unsafe {
        asm!("sti", options(nomem, nostack, preserves_flags));
    }
    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

unsafe fn detect_cpus_inner(rsdp_addr: u64) -> Option<usize> {
    let rsdp_ptr = phys_ptr::<RsdpV2>(rsdp_addr)?;
    let revision = unsafe { (*rsdp_ptr).first_part.revision };
    let table_phys = if revision >= 2 {
        let xsdt = unsafe { (*rsdp_ptr).xsdt_address };
        if xsdt != 0 {
            xsdt
        } else {
            unsafe { (*rsdp_ptr).first_part.rsdt_address as u64 }
        }
    } else {
        unsafe { (*rsdp_ptr).first_part.rsdt_address as u64 }
    };

    if table_phys == 0 {
        return None;
    }

    let sdt_header = phys_ptr::<AcpiSdtHeader>(table_phys)?;
    let table_len = unsafe { (*sdt_header).length as usize };
    let entry_size = if revision >= 2 { 8 } else { 4 };
    if table_len < SDT_HEADER_SIZE {
        return None;
    }

    let entries = (table_len - SDT_HEADER_SIZE) / entry_size;
    let table_base = vmm::phys_to_virt_addr(table_phys).ok()?.as_u64();

    for index in 0..entries {
        let entry_phys = if entry_size == 8 {
            let ptr = (table_base + SDT_HEADER_SIZE as u64 + (index * 8) as u64) as *const u64;
            unsafe { ptr.read_unaligned() }
        } else {
            let ptr = (table_base + SDT_HEADER_SIZE as u64 + (index * 4) as u64) as *const u32;
            unsafe { ptr.read_unaligned() as u64 }
        };

        let candidate = phys_ptr::<AcpiSdtHeader>(entry_phys)?;
        if unsafe { &(*candidate).signature } == ACPI_MADT_SIGNATURE {
            return unsafe { parse_madt(entry_phys) };
        }
    }

    None
}

unsafe fn parse_madt(madt_phys: u64) -> Option<usize> {
    let header = phys_ptr::<AcpiSdtHeader>(madt_phys)?;
    let madt_len = unsafe { (*header).length as usize };
    if madt_len < SDT_HEADER_SIZE + MADT_HEADER_EXTRA {
        return None;
    }

    let madt_virt = vmm::phys_to_virt_addr(madt_phys).ok()?.as_u64();
    let mut offset = SDT_HEADER_SIZE + MADT_HEADER_EXTRA;
    let mut count = 0usize;
    let mut mailbox_base = 0u64;

    unsafe {
        CPU_LIST = [EMPTY_CPU_INFO; MAX_CPUS];
    }

    while offset + 2 <= madt_len && count < MAX_CPUS {
        let entry = (madt_virt + offset as u64) as *const MadtEntryHeader;
        let entry_type = unsafe { (*entry).entry_type };
        let length = unsafe { (*entry).length as usize };
        if length < 2 || offset + length > madt_len {
            break;
        }

        if entry_type == 0 && length >= 8 {
            let base = (madt_virt + offset as u64) as *const u8;
            let processor_id = unsafe { base.add(2).read_unaligned() };
            let apic_id = unsafe { base.add(3).read_unaligned() };
            let flags_ptr = unsafe { base.add(4) as *const u32 };
            let flags = unsafe { flags_ptr.read_unaligned() };
            let enabled = (flags & 1) != 0;

            if enabled {
                unsafe {
                    CPU_LIST[count] = CpuInfo {
                        apic_id,
                        processor_id,
                        enabled,
                        mailbox_addr: if mailbox_base != 0 {
                            mailbox_base + (processor_id as u64 * 2048)
                        } else {
                            0
                        },
                    };
                }
                count += 1;
            }
        } else if entry_type == 16 && length >= 16 {
            let base = (madt_virt + offset as u64) as *const u8;
            let version_ptr = unsafe { base.add(4) as *const u32 };
            let version = unsafe { version_ptr.read_unaligned() };
            let mailbox_ptr = unsafe { base.add(8) as *const u64 };
            let candidate = unsafe { mailbox_ptr.read_unaligned() };
            if version == PARKING_PROTOCOL_VERSION {
                mailbox_base = candidate;
            }
        }

        offset += length;
    }

    if count == 0 {
        unsafe {
            CPU_LIST[0] = CpuInfo {
                apic_id: bsp_apic_id(),
                processor_id: 0,
                enabled: true,
                mailbox_addr: 0,
            };
        }
        Some(1)
    } else {
        Some(count)
    }
}

unsafe fn phys_ptr<T>(phys: u64) -> Option<*const T> {
    let virt = vmm::phys_to_virt_addr(phys).ok()?;
    Some(virt.as_u64() as *const T)
}

fn allocate_ap_stack_top() -> Option<u64> {
    let mut base_phys = 0u64;

    for i in 0..4u64 {
        match pmm::alloc_frame() {
            Ok(frame) => {
                let phys = frame.start_address().as_u64();
                if i == 0 {
                    base_phys = phys;
                }
                let page = Page::<Size4KiB>::containing_address(VirtAddr::new(phys));
                let phys_frame = frame;
                vmm::map_page(
                    page,
                    phys_frame,
                    PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                )
                .ok();
            }
            Err(_) => return None,
        }
    }

    Some(base_phys + 4 * 4096)
}

fn wait_for_ap_ready(apic_id: u8) -> bool {
    for _ in 0..10_000_000u32 {
        let ready =
            unsafe { core::ptr::read_volatile((0x8290 + apic_id as usize) as *const u8) };
        if ready != 0 {
            return true;
        }
        core::hint::spin_loop();
    }

    false
}

fn start_ap_sipi(apic_id: u8) {
    let dest = (apic_id as u32) << 24;
    let sipi_vec = 0x00004600u32 | ((TRAMPOLINE_ADDR >> 12) & 0xFF);
    unsafe {
        wait_icr_idle();
        write_apic(APIC_ICR_HIGH, dest);
        write_apic(APIC_ICR_LOW, sipi_vec);
        wait_icr_idle();
    }
    busy_wait(200_000);
    unsafe {
        wait_icr_idle();
        write_apic(APIC_ICR_HIGH, dest);
        write_apic(APIC_ICR_LOW, sipi_vec);
        wait_icr_idle();
    }
}

fn start_ap_parking_protocol(apic_id: u8, mailbox_phys: u64) {
    let page_addr = mailbox_phys & !0xfffu64;
    let page = Page::<Size4KiB>::containing_address(VirtAddr::new(page_addr));
    let frame = match PhysFrame::from_start_address(PhysAddr::new(page_addr)) {
        Ok(frame) => frame,
        Err(_) => {
            start_ap_sipi(apic_id);
            return;
        }
    };
    let _ = vmm::map_page(
        page,
        frame,
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
    );

    unsafe {
        let mailbox = mailbox_phys as *mut ParkingMailbox;
        core::ptr::write_volatile(&mut (*mailbox).processor_id, apic_id as u32);
        core::ptr::write_volatile(
            &mut (*mailbox).wakeup_vector,
            ap_entry_parking as *const () as u64,
        );
        asm!("mfence", options(nomem, nostack, preserves_flags));
    }

    unsafe {
        let dest = (apic_id as u32) << 24;
        wait_icr_idle();
        write_apic(APIC_ICR_HIGH, dest);
        write_apic(APIC_ICR_LOW, 0x000000FF);
        wait_icr_idle();
    }
}

fn write_trampoline_u64(data_ptr: *mut u8, offset: usize, value: u64) {
    unsafe {
        ptr::write_unaligned(data_ptr.add(offset) as *mut u64, value);
    }
}

fn write_trampoline_u8(data_ptr: *mut u8, offset: usize, value: u8) {
    unsafe {
        ptr::write(data_ptr.add(offset), value);
    }
}

fn busy_wait(iterations: u64) {
    for _ in 0..iterations {
        core::hint::spin_loop();
    }
}


unsafe fn read_apic_smp(offset: u32) -> u32 {
    let ptr = (APIC_BASE + offset as u64) as *const u32;
    ptr.read_volatile()
}

unsafe fn wait_icr_idle() {
    // Wait for delivery status bit (bit 12) to clear
    for _ in 0..100_000u32 {
        let low = read_apic_smp(APIC_ICR_LOW);
        if low & (1 << 12) == 0 {
            return;
        }
        core::arch::asm!("pause", options(nomem, nostack));
    }
}

unsafe fn write_apic(offset: u32, value: u32) {
    let ptr = (APIC_BASE + offset as u64) as *mut u32;
    unsafe { ptr.write_volatile(value) };
}

#[allow(dead_code)]
pub fn dump_cpu_list() {
    let count = cpu_count();
    serial_print("CPUs detected: ");
    serial_print_usize(count);
    serial_print("\n");
}

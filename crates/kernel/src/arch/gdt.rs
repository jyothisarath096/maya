use core::{cell::UnsafeCell, mem::MaybeUninit, sync::atomic::{AtomicBool, Ordering}};

use x86_64::{
    VirtAddr,
    instructions::{
        segmentation::{CS, DS, ES, SS, Segment},
        tables::load_tss,
    },
    structures::{
        gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
        tss::TaskStateSegment,
    },
};

use crate::{serial_print, serial_print_usize};

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;
const DOUBLE_FAULT_STACK_SIZE: usize = 4096 * 5;

struct Selectors {
    kernel_code: SegmentSelector,
    kernel_data: SegmentSelector,
    _user_pad: SegmentSelector,
    _user_code: SegmentSelector,
    _user_data: SegmentSelector,
    tss: SegmentSelector,
}

struct GdtState {
    tss: UnsafeCell<MaybeUninit<TaskStateSegment>>,
    gdt: UnsafeCell<MaybeUninit<GlobalDescriptorTable<8>>>,
    selectors: UnsafeCell<MaybeUninit<Selectors>>,
    initialized: AtomicBool,
}

unsafe impl Sync for GdtState {}

static GDT_STATE: GdtState = GdtState {
    tss: UnsafeCell::new(MaybeUninit::uninit()),
    gdt: UnsafeCell::new(MaybeUninit::uninit()),
    selectors: UnsafeCell::new(MaybeUninit::uninit()),
    initialized: AtomicBool::new(false),
};
static mut DOUBLE_FAULT_STACK: [u8; DOUBLE_FAULT_STACK_SIZE] = [0; DOUBLE_FAULT_STACK_SIZE];

pub fn init() {
    unsafe {
        if !GDT_STATE.initialized.load(Ordering::Acquire) {
            let mut tss = TaskStateSegment::new();
            let stack_start = VirtAddr::from_ptr(core::ptr::addr_of!(DOUBLE_FAULT_STACK));
            let stack_end = stack_start + DOUBLE_FAULT_STACK_SIZE as u64;
            tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = stack_end;
            (*GDT_STATE.tss.get()).write(tss);
            let tss = (*GDT_STATE.tss.get()).assume_init_ref();
            let mut gdt = GlobalDescriptorTable::new();
            let kernel_code = gdt.append(Descriptor::kernel_code_segment());
            let kernel_data = gdt.append(Descriptor::kernel_data_segment());
            let user_pad = gdt.append(Descriptor::UserSegment(0));
            let user_data = gdt.append(Descriptor::user_data_segment());
            let user_code = gdt.append(Descriptor::user_code_segment());
            let tss = gdt.append(Descriptor::tss_segment(tss));

            (*GDT_STATE.gdt.get()).write(gdt);
            (*GDT_STATE.selectors.get()).write(Selectors {
                kernel_code,
                kernel_data,
                _user_pad: user_pad,
                _user_data: user_data,
                _user_code: user_code,
                tss,
            });
            GDT_STATE.initialized.store(true, Ordering::Release);
        }

        let gdt = (*GDT_STATE.gdt.get()).assume_init_ref();
        let selectors = (*GDT_STATE.selectors.get()).assume_init_ref();

        gdt.load();
        CS::set_reg(selectors.kernel_code);
        DS::set_reg(selectors.kernel_data);
        ES::set_reg(selectors.kernel_data);
        SS::set_reg(selectors.kernel_data);
        load_tss(selectors.tss);
        serial_print("GDT: kernel_code=");
        serial_print_usize(selectors.kernel_code.0 as usize);
        serial_print(" kernel_data=");
        serial_print_usize(selectors.kernel_data.0 as usize);
        serial_print("\n");
        serial_print("GDT entries loaded\n");
    }
}

const GICD_BASE: u64 = 0xFFFF_0000_0800_0000;
const GICC_BASE: u64 = 0xFFFF_0000_0801_0000;

const GICD_CTLR: u64 = GICD_BASE + 0x000;
const GICD_ISENABLER0: u64 = GICD_BASE + 0x100;
const GICD_ISENABLER1: u64 = GICD_BASE + 0x104;
const GICD_IPRIORITYR: u64 = GICD_BASE + 0x400;

const GICC_CTLR: u64 = GICC_BASE + 0x000;
const GICC_PMR: u64 = GICC_BASE + 0x004;
const GICC_IAR: u64 = GICC_BASE + 0x00C;
const GICC_EOIR: u64 = GICC_BASE + 0x010;

pub fn init() {
    let _ = GICD_ISENABLER1;
    unsafe {
        (GICD_CTLR as *mut u32).write_volatile(0);

        for i in 0..32u64 {
            ((GICD_BASE + 0x080 + i * 4) as *mut u32)
                .write_volatile(0xFFFF_FFFF);
        }

        for i in 0..256u64 {
            ((GICD_IPRIORITYR + i * 4) as *mut u32)
                .write_volatile(0xA0A0_A0A0);
        }

        (GICD_ISENABLER0 as *mut u32).write_volatile(1 << 30);
        (GICD_CTLR as *mut u32).write_volatile(1);
    }
    init_cpu_interface();
    crate::uart_print!("GIC-v2 initialised\n");
}

pub fn init_cpu_interface() {
    unsafe {
        (GICC_PMR as *mut u32).write_volatile(0xF0);
        (GICC_CTLR as *mut u32).write_volatile(1);
    }
}

pub fn handle_irq() {
    let ack = acknowledge_irq();
    let intid = ack & 0x3FF;
    if intid == 1023 {
        return;
    }
    handle_irq_id(intid);
    end_irq(ack);
}

pub fn acknowledge_irq() -> u32 {
    unsafe { (GICC_IAR as *const u32).read_volatile() }
}

pub fn end_irq(ack: u32) {
    unsafe {
        (GICC_EOIR as *mut u32).write_volatile(ack);
    }
}

pub fn handle_irq_id(intid: u32) {
    if intid == 0 {
        crate::cap::cache::cap_cache_flush_local();
    }

    if intid == 1 {
        crate::ipc::channel::handle_ipc_sgi();
    }

    if intid == 27 {
        crate::arch::timer::handle_tick();
    }
}

pub fn send_ipi(target_cpu: u64) {
    const GICD_SGIR: u64 = GICD_BASE + 0xF00;
    unsafe {
        let val = ((1u32 << target_cpu) << 16) | 1u32;
        (GICD_SGIR as *mut u32).write_volatile(val);
    }
}

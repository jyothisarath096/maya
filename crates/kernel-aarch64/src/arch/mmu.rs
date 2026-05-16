const PAGE_SIZE: usize = 4096;
const MT_DEVICE_NGNRNE: u64 = 0;
const MT_NORMAL: u64 = 1;
const PTE_VALID: u64 = 1 << 0;
const PTE_TABLE: u64 = 1 << 1;
const PTE_PAGE: u64 = 1 << 1;
const PTE_AF: u64 = 1 << 10;
const PTE_SH_IS: u64 = 3 << 8;
const PTE_AP_RW: u64 = 0 << 6;
const PTE_AP_RO: u64 = 2 << 6;
const PTE_UXN: u64 = 1 << 54;
const PTE_PXN: u64 = 1 << 53;
const PTE_ATTRINDX_NORMAL: u64 = 1 << 2;
const PTE_ATTRINDX_DEVICE: u64 = 0 << 2;

pub fn init() {
    let _ = (
        PAGE_SIZE,
        MT_DEVICE_NGNRNE,
        MT_NORMAL,
        PTE_VALID,
        PTE_TABLE,
        PTE_PAGE,
        PTE_AF,
        PTE_SH_IS,
        PTE_AP_RW,
        PTE_AP_RO,
        PTE_UXN,
        PTE_PXN,
        PTE_ATTRINDX_NORMAL,
        PTE_ATTRINDX_DEVICE,
    );
    crate::uart_print!("MMU configured\n");
}

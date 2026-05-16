ENTRY(_start)

KERNEL_PHYS_BASE = 0x40200000;
KERNEL_VIRT_BASE = 0xFFFF000040200000;
PHYS_TO_VIRT_OFFSET = KERNEL_VIRT_BASE - KERNEL_PHYS_BASE;

SECTIONS {
  . = KERNEL_VIRT_BASE;

  .text.boot : AT(ADDR(.text.boot) - PHYS_TO_VIRT_OFFSET) {
    *(.text.boot)
  }

  .text : AT(ADDR(.text) - PHYS_TO_VIRT_OFFSET) {
    *(.text .text.*)
  }

  .rodata : AT(ADDR(.rodata) - PHYS_TO_VIRT_OFFSET) {
    *(.rodata .rodata.*)
  }

  .data : AT(ADDR(.data) - PHYS_TO_VIRT_OFFSET) {
    *(.data .data.*)
  }

  .bss : AT(ADDR(.bss) - PHYS_TO_VIRT_OFFSET) {
    __bss_start = .;
    *(.bss .bss.*)
    . = ALIGN(8);
    __bss_end = .;
  }

  . = ALIGN(4096);
}

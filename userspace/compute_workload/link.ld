ENTRY(_start)

PHDRS {
  load PT_LOAD FLAGS(7);
}

SECTIONS {
  . = 0x41800000;

  .text : { *(.text .text.*) } :load
  .rodata : { *(.rodata .rodata.*) } :load
  .data : { *(.data .data.*) } :load
  .bss : {
    __bss_start = .;
    *(.bss .bss.*)
    __bss_end = .;
  } :load
}

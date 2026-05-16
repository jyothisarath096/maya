pub const CMD_GET_DISPLAY_INFO: u32 = 0x0100;
pub const CMD_RESOURCE_CREATE_2D: u32 = 0x0101;
pub const CMD_RESOURCE_UNREF: u32 = 0x0102;
pub const CMD_SET_SCANOUT: u32 = 0x0103;
pub const CMD_RESOURCE_FLUSH: u32 = 0x0104;
pub const CMD_TRANSFER_TO_HOST_2D: u32 = 0x0105;
pub const CMD_RESOURCE_ATTACH_BACKING: u32 = 0x0106;
pub const CMD_RESOURCE_DETACH_BACKING: u32 = 0x0107;

pub const RESP_OK_NODATA: u32 = 0x1100;
pub const RESP_OK_DISPLAY_INFO: u32 = 0x1101;

pub const FORMAT_XRGB8888: u32 = 0x0001;
pub const FORMAT_BGRX8888: u32 = 0x0002;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GpuCtrlHdr {
    pub cmd_type: u32,
    pub flags: u32,
    pub fence_id: u64,
    pub ctx_id: u32,
    pub ring_idx: u8,
    pub _pad: [u8; 3],
}

impl GpuCtrlHdr {
    pub const fn new(cmd: u32) -> Self {
        Self {
            cmd_type: cmd,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_idx: 0,
            _pad: [0; 3],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GpuRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GpuResourceCreate2D {
    pub hdr: GpuCtrlHdr,
    pub resource_id: u32,
    pub format: u32,
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GpuResourceAttachBacking {
    pub hdr: GpuCtrlHdr,
    pub resource_id: u32,
    pub nr_entries: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GpuMemEntry {
    pub addr: u64,
    pub length: u32,
    pub _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GpuSetScanout {
    pub hdr: GpuCtrlHdr,
    pub r: GpuRect,
    pub scanout_id: u32,
    pub resource_id: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GpuTransferToHost2D {
    pub hdr: GpuCtrlHdr,
    pub r: GpuRect,
    pub offset: u64,
    pub resource_id: u32,
    pub _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GpuResourceFlush {
    pub hdr: GpuCtrlHdr,
    pub r: GpuRect,
    pub resource_id: u32,
    pub _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GpuRespOkNodata {
    pub hdr: GpuCtrlHdr,
}

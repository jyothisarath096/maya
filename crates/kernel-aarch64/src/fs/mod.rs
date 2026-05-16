pub mod namespace;
pub mod store;

pub use namespace::{insert_path, list_dir, lookup_path, mkdir};
pub use store::{
    alloc_file,
    file_exists,
    query_files_by_intent,
    query_files_by_tag,
    read_file_copy,
    tag_file,
    write_file,
    FileId,
    FileIntent,
};

pub fn init() {
    namespace::mkdir(b"/").ok();
    namespace::mkdir(b"/proc").ok();
    namespace::mkdir(b"/data").ok();
    namespace::mkdir(b"/sys").ok();
    namespace::mkdir(b"/sys/compute").ok();
    namespace::mkdir(b"/sys/io").ok();
    namespace::mkdir(b"/sys/realtime").ok();
    namespace::mkdir(b"/sys/background").ok();
    namespace::mkdir(b"/sys/system").ok();
    namespace::mkdir(b"/sys/unknown").ok();
    let now = crate::arch::timer::cntpct_to_ns(crate::arch::timer::read_cntpct());
    if let Some(fid) = store::alloc_virtual(0, store::VFILE_SCHED, now) {
        namespace::insert_path(b"/proc/sched", fid, false).ok();
    }
    if let Some(fid) = store::alloc_virtual(0, store::VFILE_FS_INFO, now) {
        namespace::insert_path(b"/proc/fs", fid, false).ok();
    }
    crate::uart_print!("MayaFS: initialized\n");
}

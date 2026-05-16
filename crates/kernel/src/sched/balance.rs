use crate::sched::queue;

pub fn assign_core_for_new_process() -> u8 {
    let mut best_core = 0u8;
    let mut min_load = usize::MAX;

    for core_id in 0..16u8 {
        if !queue::core_is_online(core_id) {
            continue;
        }
        let load = queue::process_count_for_core(core_id);
        if load < min_load {
            min_load = load;
            best_core = core_id;
        }
    }

    best_core
}

pub fn rebalance() {
    let mut max_load = 0usize;
    let mut min_load = usize::MAX;
    let mut heavy_core = 0u8;
    let mut light_core = 0u8;

    for core_id in 0..16u8 {
        if !queue::core_is_online(core_id) {
            continue;
        }
        let load = queue::process_count_for_core(core_id);
        if load > max_load {
            max_load = load;
            heavy_core = core_id;
        }
        if load < min_load {
            min_load = load;
            light_core = core_id;
        }
    }

    if max_load > min_load + 2 {
        queue::migrate_one_process(heavy_core, light_core);
    }
}

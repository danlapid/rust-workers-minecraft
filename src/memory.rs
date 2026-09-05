// Emscripten's compat/malloc.h uses ten size_t fields, matching dlmalloc.
// These counters include allocator metadata and capacity inside live containers.
#[repr(C)]
struct MallInfo {
    arena: usize,
    ordblks: usize,
    smblks: usize,
    hblks: usize,
    hblkhd: usize,
    usmblks: usize,
    fsmblks: usize,
    uordblks: usize,
    fordblks: usize,
    keepcost: usize,
}

unsafe extern "C" {
    fn mallinfo() -> MallInfo;
}

pub fn heap_usage() -> [(&'static str, usize); 3] {
    // SAFETY: This target links Emscripten's dlmalloc. The C ABI and field order
    // above match its header; mallinfo only reads allocator bookkeeping.
    let info = unsafe { mallinfo() };
    [
        ("heap_allocated_bytes", info.uordblks),
        ("heap_free_bytes", info.fordblks),
        ("heap_arena_bytes", info.arena),
    ]
}
